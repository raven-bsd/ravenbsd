// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD at donor revision 133344405f532bd54ff3629a77d4c4e062f01fdb:
//   sys/netinet/tcp_stacks/rack.c:2405   rack_log_dsack_event
//   sys/netinet/tcp_stacks/rack.c:9020   rack_log_sack_passed
//   sys/netinet/tcp_stacks/rack.c:9280   rack_proc_sack_blk
//   sys/netinet/tcp_stacks/rack.c:10055  rack_peer_reneges
//   sys/netinet/tcp_stacks/rack.c:10182  rack_process_to_cumack
//   sys/netinet/tcp_stacks/rack.c:10602  rack_note_dsack
// and sys/netinet/tcp_stacks/sack_filter.c (sack_filter_blks/clear, BSD-2).
// BSD-2. The kernel `tqhash`/`rack_sendmap` and lock pointers are replaced by a range sendmap;
// the SACK/DSACK state transitions and sequence comparisons are preserved.

//! RACK SACK/DSACK/reneging group for the full RACK port.
//!
//! This module is the K336 donor-function group. It sits on the merged K313 HPTS/timer, K314 OS
//! context and K318 vtable types; it does not edit `stack.rs`, `tcp_state.rs` or `tcp_data.rs`.
//! The FreeBSD `tqhash` sendmap is represented by an ordered range map; the range split/merge
//! semantics, ACKED/SACK_PASSED/WAS_LOST flags, DSACK accounting and cumack pruning are ported.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::RackStack;
use crate::rack_types::{
    RACK_ACKED, RACK_RWND_COLLAPSED, RACK_SACK_PASSED, RACK_WAS_LOST, RACK_WAS_SACKPASS,
    RackSendmap,
};
use crate::rack_vtable::RackPortState;
use crate::seq::{seq_ge, seq_gt, seq_le, seq_lt};

/// FreeBSD `RACK_DSACK_HANDLING`.
pub const RACK_DSACK_HANDLING: u32 = 64;

/// The K336 SACK/DSACK context over the K313/K314/K318 types.
pub struct RackSack {
    /// K318 stack/vtable context (contains the K314 OS and K313 HPTS/pacing state).
    pub stack: RackStack,
    /// Range sendmap keyed by `r_start`.
    pub sendmap: BTreeMap<u32, RackSendmap>,
    /// SACKed ranges, sorted/merged.
    pub sacked_ranges: Vec<(u32, u32)>,
    /// `rack_sf` SACK-filter ranges.
    pub sack_filter: Vec<(u32, u32)>,
    /// `rack_calc_thresh_rack` result supplied by its owner (K269); microseconds.
    pub threshold_us: u32,
    /// `rc_dsack_round_seen`.
    pub dsack_round_seen: bool,
    /// `dsack_round_end`.
    pub dsack_round_end: u32,
    /// `dsack_persist`.
    pub dsack_persist: u8,
    /// `dsack_byte_cnt`.
    pub dsack_byte_cnt: u32,
    /// `retran_during_recovery`.
    pub retran_during_recovery: u32,
    /// `IN_FASTRECOVERY(t_flags)`.
    pub in_fast_recovery: bool,
    /// `r_might_revert`.
    pub might_revert: bool,
    /// The delegated recovery hook would run (`rack_handle_might_revert`).
    pub spurious_recovery: bool,
    /// `rc_last_tlp_acked_start/end`.
    pub last_tlp_acked: Option<(u32, u32)>,
    /// `rc_last_tlp_past_cumack`.
    pub last_tlp_past_cumack: bool,
    /// `rc_last_sent_tlp_seq/len`.
    pub last_sent_tlp: Option<(u32, u32)>,
    /// `rc_last_sent_tlp_past_cumack`.
    pub last_sent_tlp_past_cumack: bool,
    /// `rc_last_cumack_advance`.
    pub last_cumack_advance: u64,
    /// Last K314 log event.
    pub last_event: Option<u32>,
    /// Last DSACK log words.
    pub last_dsack_log: Option<[u32; 4]>,
}

impl Default for RackSack {
    fn default() -> Self {
        Self::new()
    }
}

impl RackSack {
    /// Create a running SACK context.
    #[must_use]
    pub fn new() -> Self {
        let mut stack = RackStack::new();
        stack.state = RackPortState::Running;
        Self {
            stack,
            sendmap: BTreeMap::new(),
            sacked_ranges: Vec::new(),
            sack_filter: Vec::new(),
            threshold_us: 0,
            dsack_round_seen: false,
            dsack_round_end: 0,
            dsack_persist: 0,
            dsack_byte_cnt: 0,
            retran_during_recovery: 0,
            in_fast_recovery: false,
            might_revert: false,
            spurious_recovery: false,
            last_tlp_acked: None,
            last_tlp_past_cumack: false,
            last_sent_tlp: None,
            last_sent_tlp_past_cumack: false,
            last_cumack_advance: 0,
            last_event: None,
            last_dsack_log: None,
        }
    }

    /// Insert a sendmap range (test/oracle setup helper).
    pub fn insert_segment(&mut self, start: u32, end: u32, last_sent_us: u32, rtr_cnt: u8) {
        if seq_lt(start, end) {
            let mut segment = RackSendmap::new(start, end, u64::from(last_sent_us));
            segment.rtr_cnt = rtr_cnt;
            self.sendmap.insert(start, segment);
        }
    }

    /// Total SACKed bytes in the range set.
    #[must_use]
    pub fn sacked_bytes(&self) -> u32 {
        self.sacked_ranges.iter().fold(0u32, |sum, (start, end)| {
            sum.saturating_add(end.wrapping_sub(*start))
        })
    }

    /// The donor `rack_log_dsack_event`; the K314 log carries four words.
    pub fn rack_log_dsack_event(&mut self, mod_: u8, flex4: u32, flex5: u32, flex6: u32) -> bool {
        let words = [u32::from(mod_), flex4, flex5, flex6];
        let logged = self.stack.os.tcp_log_event(RACK_DSACK_HANDLING, words, 0);
        self.last_event = Some(RACK_DSACK_HANDLING);
        self.last_dsack_log = Some(words);
        logged
    }

    /// Donor `sack_filter_blks`: keep blocks above `ack`, merged.
    pub fn sack_filter_blks(&mut self, blocks: &[(u32, u32)], ack: u32) -> usize {
        self.sack_filter.clear();
        for (start, end) in blocks {
            let start = if seq_lt(*start, ack) { ack } else { *start };
            if seq_lt(start, *end) {
                self.sack_filter.push((start, *end));
            }
        }
        self.sack_filter.sort_unstable();
        self.sack_filter = merge_ranges(core::mem::take(&mut self.sack_filter));
        self.sack_filter.len()
    }

    /// Donor `sack_filter_clear`.
    pub fn sack_filter_clear(&mut self, _ack: u32) {
        self.sack_filter.clear();
    }

    /// Donor `rack_log_sack_passed`: mark older unacked ranges lost at the RTT threshold and
    /// record SACK_PASSED, stopping at an already-passed range.
    pub fn rack_log_sack_passed(&mut self, acked_start: u32, cts: u32) -> u32 {
        let threshold = self.threshold_us;
        let mut lost = 0u32;
        let mut events: Vec<(u32, u32)> = Vec::new();
        let keys: Vec<u32> = self.sendmap.keys().copied().collect();
        for key in keys.into_iter().rev() {
            let Some(segment) = self.sendmap.get_mut(&key) else {
                continue;
            };
            if !seq_lt(segment.start, acked_start) {
                continue;
            }
            if segment.flags & (RACK_ACKED | RACK_RWND_COLLAPSED) != 0 {
                continue;
            }
            if segment.flags & RACK_WAS_LOST == 0
                && (segment.last_sent() as u32).saturating_add(threshold) <= cts
            {
                segment.flags |= RACK_WAS_LOST;
                lost = lost.saturating_add(segment.len());
            }
            if segment.flags & RACK_SACK_PASSED != 0 {
                break;
            }
            events.push((segment.start, segment.end));
            segment.flags |= RACK_SACK_PASSED;
            segment.flags &= !RACK_WAS_SACKPASS;
        }
        self.stack.considered_lost = self.stack.considered_lost.saturating_add(lost);
        for (start, end) in events {
            self.rack_log_dsack_event(12, 0, start, end);
        }
        lost
    }

    /// Donor `rack_proc_sack_blk`: mark a SACK block ACKED, split/merge the sendmap and return
    /// the newly SACKed byte count.
    pub fn rack_proc_sack_blk(&mut self, start: u32, end: u32, cts: u32, _segsiz: u32) -> u32 {
        if !seq_lt(start, end) {
            return 0;
        }
        let snd_una = self.stack.tp.snd_una;
        let snd_max = self.stack.tp.snd_max;
        if seq_lt(end, snd_una) || seq_gt(start, snd_max) {
            return 0;
        }
        let before = self.sacked_bytes();
        let mut overlapped = false;
        let keys: Vec<u32> = self.sendmap.keys().copied().collect();
        for key in keys {
            let Some(segment) = self.sendmap.remove(&key) else {
                continue;
            };
            if !seq_lt(segment.start, end) || !seq_lt(start, segment.end) {
                self.sendmap.insert(key, segment);
                continue;
            }
            overlapped = true;
            let split_start = if seq_lt(segment.start, start) {
                start
            } else {
                segment.start
            };
            let split_end = if seq_lt(end, segment.end) {
                end
            } else {
                segment.end
            };
            if seq_lt(segment.start, split_start) {
                let head = RackSendmap {
                    start: segment.start,
                    end: split_start,
                    ..segment
                };
                self.sendmap.insert(head.start, head);
            }
            let mut acked = RackSendmap {
                start: split_start,
                end: split_end,
                ..segment
            };
            acked.flags |= RACK_ACKED;
            self.sendmap.insert(acked.start, acked);
            if seq_lt(split_end, segment.end) {
                let tail = RackSendmap {
                    start: split_end,
                    end: segment.end,
                    flags: segment.flags & !(RACK_ACKED | RACK_SACK_PASSED),
                    ..segment
                };
                self.sendmap.insert(tail.start, tail);
            }
        }
        if !overlapped {
            return 0;
        }
        self.sacked_ranges.push((start, end));
        self.sacked_ranges = merge_ranges(core::mem::take(&mut self.sacked_ranges));
        let changed = self.sacked_bytes().saturating_sub(before);
        self.stack.sacked = self.sacked_bytes();
        let _ = self.rack_log_sack_passed(start, cts);
        changed
    }

    /// Donor `rack_peer_reneges`: clear SACKed ranges below the reneged cumack and reset the
    /// filter; the sendmap entries stay for a later loss/retransmit decision.
    pub fn rack_peer_reneges(&mut self, th_ack: u32) -> u32 {
        let mut removed = 0u32;
        let mut keep = Vec::new();
        for (start, end) in core::mem::take(&mut self.sacked_ranges) {
            if seq_le(end, th_ack) {
                removed = removed.saturating_add(end.wrapping_sub(start));
                for segment in self.sendmap.values_mut() {
                    if seq_lt(segment.start, end) && seq_lt(start, segment.end) {
                        segment.flags &= !(RACK_ACKED | RACK_SACK_PASSED | RACK_WAS_SACKPASS);
                    }
                }
            } else if seq_lt(start, th_ack) {
                removed = removed.saturating_add(th_ack.wrapping_sub(start));
                for segment in self.sendmap.values_mut() {
                    if seq_lt(segment.start, th_ack) && seq_le(start, segment.start) {
                        segment.flags &= !(RACK_ACKED | RACK_SACK_PASSED | RACK_WAS_SACKPASS);
                    }
                }
                keep.push((th_ack, end));
            } else {
                keep.push((start, end));
            }
        }
        self.sacked_ranges = merge_ranges(keep);
        self.stack.sacked = self.sacked_bytes();
        self.sack_filter_clear(th_ack);
        removed
    }

    /// Donor `rack_process_to_cumack`: prune the filter, age TLP markers, free/trim the sendmap
    /// below `th_ack` and advance `snd_una`.
    pub fn rack_process_to_cumack(&mut self, th_ack: u32, _cts: u32, acktime: u64) -> u32 {
        self.sack_filter_blks(&[], th_ack);
        if seq_gt(th_ack, self.stack.tp.snd_una) {
            self.last_cumack_advance = acktime;
        }
        self.age_tlp(th_ack);

        let mut freed = 0u32;
        let keys: Vec<u32> = self.sendmap.keys().copied().collect();
        for key in keys {
            let Some(segment) = self.sendmap.get(&key).copied() else {
                continue;
            };
            if seq_le(segment.end, th_ack) {
                self.sendmap.remove(&key);
                freed = freed.saturating_add(segment.len());
            } else if seq_lt(segment.start, th_ack) {
                self.sendmap.remove(&key);
                let trimmed = RackSendmap {
                    start: th_ack,
                    ..segment
                };
                freed = freed.saturating_add(th_ack.wrapping_sub(segment.start));
                self.sendmap.insert(trimmed.start, trimmed);
            }
        }
        self.sacked_ranges = trim_ranges(core::mem::take(&mut self.sacked_ranges), th_ack);
        self.stack.sacked = self.sacked_bytes();
        if seq_gt(th_ack, self.stack.tp.snd_una) {
            self.stack.tp.snd_una = th_ack;
        }
        self.stack.wanted_output = true;
        freed
    }

    fn age_tlp(&mut self, th_ack: u32) {
        if let Some((start, end)) = self.last_tlp_acked {
            if self.last_tlp_past_cumack && seq_gt(start, th_ack) {
                let _ = self.rack_log_dsack_event(9, 0, start, end);
                self.last_tlp_acked = None;
                self.last_tlp_past_cumack = false;
            } else if !self.last_tlp_past_cumack && seq_ge(th_ack, end) {
                self.last_tlp_past_cumack = true;
            }
        }
        if let Some((start, len)) = self.last_sent_tlp {
            let end = start.wrapping_add(len);
            if self.last_sent_tlp_past_cumack && seq_gt(start, th_ack) {
                let _ = self.rack_log_dsack_event(9, 1, start, end);
                self.last_sent_tlp = None;
                self.last_sent_tlp_past_cumack = false;
            } else if !self.last_sent_tlp_past_cumack && seq_ge(th_ack, start) {
                self.last_sent_tlp_past_cumack = true;
            }
        }
    }

    /// Donor `rack_note_dsack`: exclude TLP-caused DSACKs, count one DSACK round, add the byte
    /// count and request the delegated spurious-recovery hook when the retransmitted bytes are
    /// covered.
    pub fn rack_note_dsack(&mut self, start: u32, end: u32) -> bool {
        let amount = if seq_gt(end, start) {
            end.wrapping_sub(start)
        } else {
            0
        };
        let mut was_tlp = false;
        if let Some((tlp_start, tlp_end)) = self.last_tlp_acked
            && seq_ge(start, tlp_start)
            && seq_le(end, tlp_end)
        {
            let _ = self.rack_log_dsack_event(7, 0, start, end);
            was_tlp = true;
        }
        if !was_tlp
            && let Some((tlp_start, tlp_len)) = self.last_sent_tlp
            && seq_ge(start, tlp_start)
            && seq_le(end, tlp_start.wrapping_add(tlp_len))
        {
            let _ = self.rack_log_dsack_event(7, 1, start, end);
            was_tlp = true;
        }
        if !was_tlp && !self.dsack_round_seen {
            self.dsack_round_seen = true;
            self.dsack_round_end = self.stack.tp.snd_max;
            self.stack.num_dsacks = self.stack.num_dsacks.saturating_add(1);
            self.dsack_persist = 16;
            let _ = self.rack_log_dsack_event(2, 0, 0, 0);
        }
        self.dsack_byte_cnt = self.dsack_byte_cnt.saturating_add(amount);
        if !self.in_fast_recovery
            && self.retran_during_recovery != 0
            && self.dsack_byte_cnt >= self.retran_during_recovery
        {
            self.might_revert = true;
            self.spurious_recovery = true;
            self.retran_during_recovery = 0;
            self.dsack_byte_cnt = 0;
        }
        was_tlp
    }
}

fn merge_ranges(mut ranges: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    ranges.retain(|(start, end)| seq_lt(*start, *end));
    ranges.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        if let Some((_, last_end)) = merged.last_mut()
            && seq_le(start, *last_end)
        {
            if seq_gt(end, *last_end) {
                *last_end = end;
            }
        } else {
            merged.push((start, end));
        }
    }
    merged
}

fn trim_ranges(ranges: Vec<(u32, u32)>, ack: u32) -> Vec<(u32, u32)> {
    let mut kept = Vec::new();
    for (start, end) in ranges {
        if seq_le(end, ack) {
            continue;
        }
        let start = if seq_lt(start, ack) { ack } else { start };
        if seq_lt(start, end) {
            kept.push((start, end));
        }
    }
    merge_ranges(kept)
}

#[cfg(test)]
#[path = "rack_sack_ut.rs"]
mod tests;
