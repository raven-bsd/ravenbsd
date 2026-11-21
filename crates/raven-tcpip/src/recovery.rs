// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RACK-TLP loss detection, spurious-retransmission undo and ECN state.
//!
//! The `Rack` state machine ports the detection/recovery surface of FreeBSD
//! `sys/netinet/tcp_stacks/rack.c` (BSD-2-Clause): cumulative ACK processing, SACK/DSACK,
//! reneging, RTT/min-RTT, recovery entry/exit, retransmit/rescue selection, PRR accounting and
//! the reordering-window calculation. The HPTS scheduler, timer thread and socket-option glue
//! stay in the transport; every excluded path is recorded in the K269 donor-divergence table.

use alloc::vec::Vec;

use crate::Duration;
use crate::seq::{seq_ge, seq_gt, seq_le, seq_lt};

/// Number of retransmit timestamps kept per range (`RACK_NUM_OF_RETRANS`).
pub const RACK_NUM_OF_RETRANS: usize = 8;
/// Duplicate-ACK threshold (`DUP_ACK_THRESHOLD`).
pub const RACK_DUP_THRESHOLD: u8 = 3;
/// Reordering window ceiling (`rack_rto_max` default, 4 s).
pub const RACK_RTO_MAX_US: u32 = 4_000_000;
/// Reordering fade (`rack_reorder_fade` default, 60 s).
pub const RACK_REORDER_FADE_US: u64 = 60_000_000;
/// Packet delay (`rack_pkt_delay` default, 1 ms).
pub const RACK_PKT_DELAY_US: u32 = 1_000;
/// Reordering shift (`rack_reorder_thresh` default, 2).
pub const RACK_REORDER_SHIFT: u32 = 2;
/// TLP without new data (`rack_tlp_limit` default, 2).
pub const RACK_TLP_LIMIT: u16 = 2;
/// `rack_rtt_divisor` default.
pub const RACK_RTT_DIVISOR: u32 = 2;

/// One transmitted range tracked by RACK.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RackEntry {
    /// Inclusive start sequence.
    pub start: u32,
    /// Exclusive end sequence.
    pub end: u32,
    /// Latest transmission timestamp in milliseconds (compatibility view).
    pub xmit_ts: u32,
    /// Transmission timestamps in microseconds, oldest first (`r_tim_lastsent`).
    pub xmit_us: [u64; RACK_NUM_OF_RETRANS],
    /// Transmit count; 1 means the original send (`r_rtr_cnt`).
    pub rtr_count: u8,
    /// Bytes retransmitted from this range (`r_rtr_bytes`).
    pub rtr_bytes: u32,
    /// Flight at send (`r_fas`).
    pub fas: u32,
    /// Duplicate-ACK strikes (`r_dupack`).
    pub dupack: u8,
    /// Time the SACK arrived, in microseconds (`r_ack_arrival`).
    pub ack_arrival: u64,
    /// The range is SACKed.
    pub sacked: bool,
    /// The range has been retransmitted.
    pub retransmitted: bool,
    /// An older transmitted range was SACKed past this one (`RACK_SACK_PASSED`).
    pub sack_passed: bool,
    /// This range was SACKed in the past (`RACK_WAS_SACKPASS`).
    pub was_sacked: bool,
    /// Declared lost by time or SACK pressure.
    pub lost: bool,
    /// Considered lost for pipe accounting (`RACK_WAS_LOST`).
    pub was_lost: bool,
    /// Sent by a tail-loss probe (`RACK_TLP`).
    pub tlp: bool,
    /// Must be retransmitted after an RTO (`RACK_MUST_RXT`).
    pub must_rxt: bool,
}

impl Default for RackEntry {
    fn default() -> Self {
        Self {
            start: 0,
            end: 0,
            xmit_ts: 0,
            xmit_us: [0; RACK_NUM_OF_RETRANS],
            rtr_count: 1,
            rtr_bytes: 0,
            fas: 0,
            dupack: 0,
            ack_arrival: 0,
            sacked: false,
            retransmitted: false,
            sack_passed: false,
            was_sacked: false,
            lost: false,
            was_lost: false,
            tlp: false,
            must_rxt: false,
        }
    }
}

impl RackEntry {
    /// Range length in bytes.
    #[must_use]
    pub const fn len(&self) -> u32 {
        self.end.wrapping_sub(self.start)
    }

    /// Whether the range is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.end == self.start
    }

    /// Latest transmission timestamp in microseconds.
    #[must_use]
    pub fn last_xmit_us(&self) -> u64 {
        let index = self.rtr_count.max(1) as usize - 1;
        self.xmit_us[index.min(RACK_NUM_OF_RETRANS - 1)]
    }
}

/// Inputs for one RACK ACK/recovery event.
#[derive(Clone, Copy, Debug, Default)]
pub struct RackContext {
    /// Current monotonic time in milliseconds.
    pub now_ms: u32,
    /// Current monotonic time in microseconds; `0` means `now_ms * 1000`.
    pub now_us: u64,
    /// Smoothed RTT in milliseconds.
    pub srtt_ms: u32,
    /// Smoothed RTT in microseconds; `0` means `srtt_ms * 1000`.
    pub srtt_us: u32,
    /// Minimum RTT in milliseconds.
    pub min_rtt_ms: u32,
    /// Current sender MSS.
    pub mss: u32,
    /// Cumulative ACK.
    pub ack: u32,
    /// Oldest unacknowledged sequence at entry.
    pub snd_una: u32,
    /// Highest sequence sent.
    pub snd_nxt: u32,
    /// Highest sequence ever sent.
    pub snd_max: u32,
    /// Congestion window before this event.
    pub cwnd: u32,
    /// Slow-start threshold.
    pub ssthresh: u32,
    /// Receiver window.
    pub snd_wnd: u32,
    /// Current RTO in microseconds; `0` falls back to `2 * SRTT`.
    pub rto_us: u32,
    /// Segments acknowledged by this event (`t_ccv.nsegs`).
    pub nsegs: u16,
    /// The sender was cwnd limited (`CCF_CWND_LIMITED`).
    pub cwnd_limited: bool,
    /// Early-recovery ACK cap (`rc_early_recovery_segs`).
    pub early_recovery_segs: u16,
    /// Duplicate-ACK threshold.
    pub dup_thresh: u32,
    /// SACK is permitted on this connection (`TF_SACK_PERMIT`).
    pub sack_permitted: bool,
    /// The sender is application limited.
    pub app_limited: bool,
    /// Timestamp echo in milliseconds (`to_tsecr`), when present.
    pub ts_echo: Option<u32>,
    /// SACK blocks, start/end pairs.
    pub sack_count: u8,
    /// SACK blocks (`to_sacks`).
    pub sacks: [(u32, u32); 4],
    /// Explicit DSACK blocks.
    pub dsack_count: u8,
    /// DSACK blocks; blocks below the cumulative ACK are treated as DSACK too.
    pub dsacks: [(u32, u32); 4],
    /// `rack_reorder_fade` override in microseconds.
    pub reorder_fade_us: u64,
    /// `rack_reorder_thresh` override.
    pub reorder_shift: u32,
    /// `rack_pkt_delay` override in microseconds.
    pub pkt_delay_us: u32,
    /// `rc_rack_tmr_std_based` (default on, `rack_dsack_std_based = 0x3`).
    pub std_based: bool,
    /// `rc_rack_use_dsack` (default on).
    pub use_dsack: bool,
}

impl RackContext {
    /// Effective event time in microseconds.
    #[must_use]
    pub fn event_us(&self) -> u64 {
        if self.now_us != 0 {
            self.now_us
        } else {
            u64::from(self.now_ms).saturating_mul(1_000)
        }
    }

    /// Effective SRTT in microseconds.
    #[must_use]
    pub fn srtt_us(&self) -> u32 {
        if self.srtt_us != 0 {
            self.srtt_us
        } else {
            self.srtt_ms.saturating_mul(1_000).max(1)
        }
    }
}

/// Result of one RACK ACK/recovery event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RackOutcome {
    /// Bytes newly acknowledged.
    pub acked: u32,
    /// Ranges declared lost, in sequence order.
    pub lost: Vec<u32>,
    /// The recovery episode ended.
    pub recovered: bool,
    /// The sender should retransmit `lost`/`retransmit_range`.
    pub retransmit: bool,
    /// A range selected for retransmission.
    pub retransmit_range: Option<(u32, u32)>,
    /// Bytes newly SACKed.
    pub sacked: u32,
    /// A DSACK was processed.
    pub dsack: bool,
    /// Bytes the peer reneged.
    pub reneged: u32,
    /// A valid RTT sample, if any.
    pub rtt_sample_us: Option<u32>,
    /// This event entered recovery.
    pub entered_recovery: bool,
    /// The event looked spurious (DSACK equals retransmitted data).
    pub spurious: bool,
    /// PRR send allowance after the event.
    pub prr_sndcnt: u32,
    /// Bytes passed to congestion control (`t_ccv.bytes_this_ack`).
    pub cc_acked: u32,
    /// The sender was cwnd limited at this ACK.
    pub cwnd_limited: bool,
    /// ABC reached a full window (`CCF_ABC_SENTAWND`).
    pub abc_sent_awnd: bool,
    /// The ACK was a duplicate.
    pub dup_ack: bool,
    /// The ACK only partly drained recovery.
    pub partial_recovery: bool,
    /// The recovery-exit ssthresh restored by a reordering ACK.
    pub restored_ssthresh: Option<u32>,
}

/// A FreeBSD `ccsignal_t`-style RACK congestion signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackSignal {
    /// `CC_NDUPACK`: SACK/duplicate-ACK loss.
    DupAck,
    /// `CC_ECN`: ECN congestion mark.
    Ecn,
    /// `CC_RTO`: retransmission timeout.
    Rto,
    /// `CC_RTO_ERR`: the RTO was spurious.
    RtoErr,
}

/// Result of a RACK congestion signal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RackCongOutcome {
    /// Congestion window the transport should use after the signal.
    pub cwnd: u32,
    /// Slow-start threshold after the signal.
    pub ssthresh: u32,
    /// Whether recovery is active.
    pub in_recovery: bool,
    /// Recovery point.
    pub recover: u32,
    /// PRR bytes available to send.
    pub prr_sndcnt: u32,
    /// Send a CWR.
    pub send_cwr: bool,
}

/// Which RACK timer `rack_timer_start` selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackTimerKind {
    /// Retransmission timer (`PACE_TMR_RXT`).
    Rxt,
    /// RACK reordering timer (`PACE_TMR_RACK`).
    Rack,
    /// Tail-loss-probe timer (`PACE_TMR_TLP`).
    Tlp,
}

/// Sans-IO result of `rack_timer_start`; HPTS insertion is the K313 interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RackTimerPlan {
    /// Timer to arm.
    pub kind: RackTimerKind,
    /// Delay in microseconds.
    pub delay_us: u32,
    /// Donor `PACE_TMR_*` flag value.
    pub flags: u32,
}

/// `PACE_TMR_RXT` from `tcp_hpts.h`.
pub const PACE_TMR_RXT: u32 = 0x01;
/// `PACE_TMR_RACK` from `tcp_hpts.h`.
pub const PACE_TMR_RACK: u32 = 0x02;
/// `PACE_TMR_TLP` from `tcp_hpts.h`.
pub const PACE_TMR_TLP: u32 = 0x04;

/// Per-segment RACK transmission bookkeeping.
#[derive(Clone, Debug)]
pub struct Rack {
    /// Latest transmission timestamp seen, in milliseconds.
    pub xmit_ts: u32,
    /// Highest sequence covered by the latest transmission.
    pub end_seq: u32,
    /// Reordering window in milliseconds (legacy API view).
    pub reorder_window: u32,
    /// Whether a DSACK has grown the window.
    pub dsack_grown: bool,
    /// Outstanding transmitted ranges, sorted by sequence.
    pub entries: Vec<RackEntry>,
    /// Cumulative ACK at the last event.
    pub snd_una: u32,
    /// Highest sequence ever sent.
    pub snd_max: u32,
    /// Recovery point at entry.
    pub recover: u32,
    /// True while a RACK recovery episode is active.
    pub in_recovery: bool,
    /// Smallest observed RTT in milliseconds.
    pub min_rtt_ms: u32,
    /// Smallest observed RTT in microseconds (`rc_rack_min_rtt`).
    pub min_rtt_us: u32,
    /// SRTT used by the last event, in microseconds.
    pub srtt_us: u32,
    /// Latest RACK transmit time in microseconds (`rc_rack_tmit_time`).
    pub rack_tmit_us: u64,
    /// RTT associated with `rack_tmit_us`.
    pub rack_rtt_us: u32,
    /// Last time reordering was observed, in microseconds.
    pub reorder_ts_us: u64,
    /// `rack_reorder_fade` in microseconds.
    pub reorder_fade_us: u64,
    /// `rack_reorder_thresh` shift.
    pub reorder_shift: u32,
    /// `rack_pkt_delay` in microseconds.
    pub pkt_delay_us: u32,
    /// `rc_rack_tmr_std_based`.
    pub std_based: bool,
    /// `rc_rack_use_dsack`.
    pub use_dsack: bool,
    /// DSACK rounds observed since the last recovery (`num_dsack`).
    pub num_dsack: u32,
    /// Persist countdown (`dsack_persist`).
    pub dsack_persist: u16,
    /// A DSACK round is open.
    pub dsack_round_seen: bool,
    /// End of the current DSACK round.
    pub dsack_round_end: u32,
    /// DSACK bytes seen in this recovery.
    pub dsack_byte_cnt: u32,
    /// Bytes retransmitted in this recovery.
    pub retran_during_recovery: u32,
    /// The recovery may have been spurious.
    pub might_revert: bool,
    /// Retransmitted bytes still considered lost.
    pub recovery_rxt_cnt: u32,
    /// Flight size at recovery entry (`rc_prr_recovery_fs`).
    pub recover_fs: u32,
    /// PRR delivered bytes.
    pub prr_delivered: u32,
    /// PRR bytes already sent.
    pub prr_out: u32,
    /// PRR send allowance.
    pub prr_sndcnt: u32,
    /// SACKed bytes (`rc_sacked`).
    pub sacked_bytes: u32,
    /// Retransmitted bytes in holes (`rc_holes_rxt`).
    pub holes_rxt: u32,
    /// Bytes considered lost for pipe accounting (`rc_considered_lost`).
    pub considered_lost: u32,
    /// A TLP is armed.
    pub tlp_deadline_us: Option<u64>,
    /// TLPs sent without new data.
    pub tlp_count: u16,
    /// Last TLP retransmit, if any.
    pub last_sent_tlp: Option<(u32, u32)>,
    /// Last TLP-covered range that was cumulatively ACKed.
    pub last_tlp_acked: Option<(u32, u32)>,
    /// ABC bytes accumulated (`t_bytes_acked`).
    pub t_bytes_acked: u32,
    /// ssthresh at recovery entry (`rc_ssthresh_at_erec`).
    pub recover_ssthresh: u32,
    /// ssthresh before an RTO (`rto_ssthresh`).
    pub rto_ssthresh: u32,
    /// Last ssthresh restored by a reordering exit.
    pub last_ssthresh: u32,
    /// TLP threshold divisor (`rc_tlp_threshold`, default 1).
    pub tlp_threshold: u32,
    /// Snapshot for `CC_RTO_ERR`.
    undo: Option<(bool, u32, u32, u32, u32)>,
}

impl Rack {
    /// Create a RACK detector with a reordering window.
    #[must_use]
    pub const fn new(reorder_window_ms: u32) -> Self {
        Self {
            xmit_ts: 0,
            end_seq: 0,
            reorder_window: reorder_window_ms,
            dsack_grown: false,
            entries: Vec::new(),
            snd_una: 0,
            snd_max: 0,
            recover: 0,
            in_recovery: false,
            min_rtt_ms: 0,
            min_rtt_us: 0,
            srtt_us: 0,
            rack_tmit_us: 0,
            rack_rtt_us: 0,
            reorder_ts_us: 0,
            reorder_fade_us: RACK_REORDER_FADE_US,
            reorder_shift: RACK_REORDER_SHIFT,
            pkt_delay_us: RACK_PKT_DELAY_US,
            std_based: true,
            use_dsack: true,
            num_dsack: 0,
            dsack_persist: 0,
            dsack_round_seen: false,
            dsack_round_end: 0,
            dsack_byte_cnt: 0,
            retran_during_recovery: 0,
            might_revert: false,
            recovery_rxt_cnt: 0,
            recover_fs: 0,
            prr_delivered: 0,
            prr_out: 0,
            prr_sndcnt: 0,
            sacked_bytes: 0,
            holes_rxt: 0,
            considered_lost: 0,
            tlp_deadline_us: None,
            tlp_count: 0,
            last_sent_tlp: None,
            last_tlp_acked: None,
            t_bytes_acked: 0,
            recover_ssthresh: 0,
            rto_ssthresh: 0,
            last_ssthresh: 0,
            tlp_threshold: 1,
            undo: None,
        }
    }

    /// Record a transmission.
    pub fn on_sent(&mut self, end_seq: u32, now_ms: u32) {
        if now_ms >= self.xmit_ts {
            self.xmit_ts = now_ms;
            self.end_seq = end_seq;
        }
        if seq_gt(end_seq, self.snd_max) {
            self.snd_max = end_seq;
        }
    }

    /// Declare loss by time: the segment is older than the reordering window.
    #[must_use]
    pub fn is_lost(&self, now_ms: u32, srtt_ms: u32) -> bool {
        let bound = (srtt_ms / 4).max(self.reorder_window).max(1);
        now_ms.saturating_sub(self.xmit_ts) > bound
    }

    /// Grow the reordering window after a DSACK, bounded by the RTT.
    pub fn on_dsack(&mut self, srtt_ms: u32) {
        self.reorder_window = self
            .reorder_window
            .saturating_add((srtt_ms / 4).max(1))
            .min(srtt_ms.max(1));
        self.dsack_grown = true;
    }

    /// Reset after an ACK advances the recovery point.
    pub fn on_ack(&mut self, end_seq: u32, now_ms: u32) {
        self.end_seq = end_seq;
        self.xmit_ts = now_ms;
        self.tlp_deadline_us = None;
        self.tlp_count = 0;
    }

    /// Insert or update one transmitted range.
    pub fn send(&mut self, start: u32, end: u32, now_ms: u32) {
        if end <= start {
            return;
        }
        let now_us = u64::from(now_ms).saturating_mul(1_000);
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.start == start && entry.end == end)
        {
            entry.xmit_ts = now_ms;
            entry.xmit_us = [0; RACK_NUM_OF_RETRANS];
            entry.xmit_us[0] = now_us;
            entry.rtr_count = 1;
            entry.retransmitted = false;
            entry.sacked = false;
            entry.lost = false;
            entry.was_lost = false;
        } else {
            let mut entry = RackEntry {
                start,
                end,
                xmit_ts: now_ms,
                ..RackEntry::default()
            };
            entry.xmit_us[0] = now_us;
            entry.fas = self.snd_max.wrapping_sub(self.snd_una);
            self.entries.push(entry);
            self.entries.sort_by_key(|entry| entry.start);
        }
        self.on_sent(end, now_ms);
    }

    /// Mark a range SACKed, splitting entries at the block edges.
    pub fn mark_sacked(&mut self, start: u32, end: u32) {
        if end <= start {
            return;
        }
        self.split_at_seq(start);
        self.split_at_seq(end);
        for entry in &mut self.entries {
            if entry.start >= start && entry.end <= end && !entry.sacked {
                entry.sacked = true;
                entry.was_sacked = true;
                self.sacked_bytes = self.sacked_bytes.saturating_add(entry.len());
            }
        }
    }

    /// FreeBSD `rack_ack_received`/`rack_process_ack` core: cumulative ACK, SACK, DSACK,
    /// reneging, RTT, recovery entry/exit and loss selection.
    pub fn ack_received(&mut self, ctx: &RackContext) -> RackOutcome {
        self.process_ack(ctx)
    }

    /// Full `rack_process_ack` core. `ack_received` is the compatibility name.
    pub fn process_ack(&mut self, ctx: &RackContext) -> RackOutcome {
        let mut outcome = RackOutcome::default();
        let now_us = ctx.event_us().max(1);
        let old_una = self.snd_una;
        self.srtt_us = ctx.srtt_us();
        self.update_context_tuning(ctx);
        if ctx.snd_max != 0 {
            self.snd_max = ctx.snd_max;
        } else if seq_gt(ctx.snd_nxt, self.snd_max) {
            self.snd_max = ctx.snd_nxt;
        }

        if seq_gt(ctx.ack, self.snd_max) {
            return outcome;
        }
        if seq_lt(ctx.ack, self.snd_una) {
            // Old ACK: a reordering signal, but not a new cumulative point.
            if ctx.snd_wnd > ctx.mss {
                self.reorder_ts_us = now_us.max(1);
            }
            return outcome;
        }
        if seq_gt(ctx.ack, self.snd_una) {
            outcome.acked = ctx.ack.wrapping_sub(self.snd_una);
            self.process_to_cumack(ctx, ctx.ack, now_us, &mut outcome);
        }
        outcome.cc_acked = outcome.acked;
        outcome.cwnd_limited = ctx.cwnd <= ctx.snd_wnd;
        if self.in_recovery && ctx.early_recovery_segs > 0 {
            let cap = u32::from(ctx.early_recovery_segs).saturating_mul(ctx.mss);
            outcome.cc_acked = outcome.cc_acked.min(cap);
        }
        if ctx.cwnd > ctx.ssthresh {
            let abc = outcome.acked.min(
                u32::from(ctx.nsegs.max(1))
                    .saturating_mul(2)
                    .saturating_mul(ctx.mss),
            );
            self.t_bytes_acked = self.t_bytes_acked.saturating_add(abc);
            if self.t_bytes_acked >= ctx.cwnd {
                self.t_bytes_acked = self.t_bytes_acked.saturating_sub(ctx.cwnd);
                outcome.abc_sent_awnd = true;
            }
        } else {
            self.t_bytes_acked = 0;
        }
        if ctx.ack == old_una && ctx.sack_count == 0 {
            outcome.dup_ack = true;
            let _ = self.strike_dupack();
            outcome.cc_acked = 0;
        }
        if self.in_recovery && seq_lt(ctx.ack, self.recover) {
            outcome.partial_recovery = true;
        }

        let mut dsack_seen = false;
        for index in 0..usize::from(ctx.dsack_count.min(4)) {
            let (start, end) = ctx.dsacks[index];
            if seq_lt(start, end) {
                self.note_dsack(start, end, ctx, &mut outcome);
                dsack_seen = true;
            }
        }
        for index in 0..usize::from(ctx.sack_count.min(4)) {
            let (start, end) = ctx.sacks[index];
            if end <= start {
                continue;
            }
            if seq_le(end, ctx.ack) {
                self.note_dsack(start, end, ctx, &mut outcome);
                dsack_seen = true;
            } else if seq_gt(start, ctx.ack) && seq_le(end, self.snd_max) {
                self.process_sack_block(ctx, start, end, now_us, &mut outcome);
            }
        }
        outcome.dsack = dsack_seen;
        self.check_reorder_exit(ctx, now_us, &mut outcome);

        self.declare_losses(ctx, now_us, &mut outcome);
        if self.in_recovery && seq_ge(ctx.ack, self.recover) {
            let pipe = self.compute_pipe(ctx.ack);
            let _ = self.rack_post_recovery(ctx, pipe);
            outcome.recovered = true;
            outcome.restored_ssthresh = Some(self.last_ssthresh);
        }
        if outcome.acked > 0 {
            self.tlp_deadline_us = None;
            self.tlp_count = 0;
        }
        outcome.prr_sndcnt = self.prr_sndcnt;
        outcome
    }

    /// FreeBSD `rack_update_entry`: retransmit part of a range and update its history.
    pub fn update_entry(&mut self, start: u32, len: u32, now_ms: u32) {
        if len == 0 {
            return;
        }
        let now_us = u64::from(now_ms).saturating_mul(1_000);
        let mut cursor = start;
        let mut remaining = len;
        while remaining > 0 {
            if let Some(index) = self.index_for(cursor)
                && self.entries[index].start < cursor
            {
                self.split_at_seq(cursor);
            }
            let Some(index) = self.index_for(cursor) else {
                self.send(cursor, cursor.saturating_add(remaining), now_ms);
                return;
            };
            let entry_end = self.entries[index].end;
            let end = cursor.saturating_add(remaining);
            if end >= entry_end {
                let mut entry = self.entries[index];
                self.update_rsm(&mut entry, now_us, now_ms);
                self.entries[index] = entry;
                if end == entry_end {
                    return;
                }
                remaining = end - entry_end;
                cursor = entry_end;
            } else {
                self.split_at_seq(end);
                let mut entry = self.entries[index];
                self.update_rsm(&mut entry, now_us, now_ms);
                self.entries[index] = entry;
                return;
            }
        }
    }

    /// FreeBSD `rack_post_recovery`: return the recovery-exit window.
    pub fn post_recovery(&self, ctx: &RackContext, pipe: u32) -> u32 {
        if pipe < ctx.ssthresh {
            pipe.max(ctx.mss).saturating_add(ctx.mss)
        } else {
            ctx.ssthresh
        }
    }

    /// FreeBSD `rack_post_recovery`: recovery-exit window plus the PRR/DSACK/RTO bookkeeping.
    pub fn rack_post_recovery(&mut self, ctx: &RackContext, pipe: u32) -> u32 {
        let cwnd = self.post_recovery(ctx, pipe);
        if self.dsack_persist > 0 {
            self.dsack_persist -= 1;
            if self.num_dsack > 0 && self.dsack_persist == 0 {
                self.num_dsack = 0;
            }
        }
        self.last_ssthresh = ctx.ssthresh.max(self.rto_ssthresh);
        self.exit_recovery();
        cwnd
    }

    /// FreeBSD `rack_calc_thresh_tlp`.
    #[must_use]
    pub fn rack_calc_thresh_tlp(&self, ctx: &RackContext) -> u32 {
        let srtt = ctx.srtt_us().max(1);
        let thresh = if self.tlp_threshold != 0 {
            srtt.saturating_add(srtt / self.tlp_threshold.max(1))
        } else {
            srtt.saturating_mul(2)
        };
        thresh.clamp(1, RACK_RTO_MAX_US)
    }

    /// FreeBSD `rack_timer_start`: choose the next RACK/RXT/TLP timer as a sans-IO plan.
    pub fn rack_timer_start(&mut self, ctx: &RackContext) -> Option<RackTimerPlan> {
        let now_us = ctx.event_us().max(1);
        if self.entries.iter().all(|entry| entry.sacked) && self.snd_una == self.snd_max {
            return None;
        }
        if !ctx.sack_permitted {
            let delay = if ctx.rto_us != 0 {
                ctx.rto_us
            } else {
                ctx.srtt_us().saturating_mul(2).max(1)
            };
            return Some(RackTimerPlan {
                kind: RackTimerKind::Rxt,
                delay_us: delay,
                flags: PACE_TMR_RXT,
            });
        }
        if let Some(deadline) = self.tlp_deadline_us
            && !self.in_recovery
        {
            return Some(RackTimerPlan {
                kind: RackTimerKind::Tlp,
                delay_us: u32::try_from(deadline.saturating_sub(now_us))
                    .unwrap_or(u32::MAX)
                    .max(1),
                flags: PACE_TMR_TLP,
            });
        }
        let index = self.oldest_unacked_index()?;
        let entry = self.entries[index];
        let thresh = u64::from(self.reorder_threshold_us(ctx, now_us));
        let expire = entry.last_xmit_us().max(1).saturating_add(thresh);
        Some(RackTimerPlan {
            kind: RackTimerKind::Rack,
            delay_us: u32::try_from(expire.saturating_sub(now_us))
                .unwrap_or(u32::MAX)
                .max(1),
            flags: PACE_TMR_RACK,
        })
    }

    /// Update the reordering window from an observed ACK gap, bounded by SRTT.
    pub fn observe_reordering(&mut self, srtt_ms: u32) {
        if srtt_ms == 0 {
            return;
        }
        self.reorder_window = self
            .reorder_window
            .saturating_add((srtt_ms / 4).max(1))
            .min(srtt_ms);
    }

    /// FreeBSD `rack_calc_thresh_rack`: reordering window in microseconds.
    pub fn reorder_threshold_us(&mut self, ctx: &RackContext, now_us: u64) -> u32 {
        let srtt = ctx.srtt_us().max(1);
        let lro = if self.reorder_ts_us != 0 {
            if self.reorder_fade_us != 0 {
                let age = now_us.saturating_sub(self.reorder_ts_us);
                if age > self.reorder_fade_us {
                    self.reorder_ts_us = 0;
                    0
                } else {
                    1
                }
            } else {
                1
            }
        } else {
            0
        };
        let mut thresh = if self.std_based {
            srtt.saturating_add(srtt >> 2)
        } else {
            srtt.saturating_add(self.pkt_delay_us)
        };
        if lro != 0 && !self.std_based {
            thresh = thresh.saturating_add(if self.reorder_shift != 0 {
                srtt >> self.reorder_shift.min(31)
            } else {
                srtt >> 2
            });
        }
        if self.use_dsack && lro != 0 && self.num_dsack > 0 {
            thresh = thresh.saturating_add(self.num_dsack.saturating_mul(srtt >> 2));
        }
        thresh = thresh.min(srtt.saturating_mul(2));
        thresh.clamp(1, RACK_RTO_MAX_US)
    }

    /// FreeBSD `rack_update_rtt`: update min/RACK RTT and report a valid sample.
    pub fn update_rtt(&mut self, entry: &RackEntry, ctx: &RackContext, now_us: u64) -> Option<u32> {
        if entry.rtr_count <= 1 {
            let sample = now_us
                .saturating_sub(entry.xmit_us[0])
                .max(1)
                .min(u64::from(u32::MAX)) as u32;
            self.latch_rtt(sample, entry.xmit_us[0]);
            return Some(sample);
        }
        if let Some(echo) = ctx.ts_echo {
            for index in 0..usize::from(entry.rtr_count.min(RACK_NUM_OF_RETRANS as u8)) {
                if entry.xmit_us[index] / 1_000 == u64::from(echo) {
                    if index + 1 < usize::from(entry.rtr_count) {
                        // The peer acked a previous transmission: spurious retransmit.
                        return None;
                    }
                    let sample = now_us
                        .saturating_sub(entry.xmit_us[index])
                        .max(1)
                        .min(u64::from(u32::MAX)) as u32;
                    self.latch_rtt(sample, entry.xmit_us[index]);
                    return Some(sample);
                }
            }
        }
        let sample = now_us
            .saturating_sub(entry.last_xmit_us())
            .max(1)
            .min(u64::from(u32::MAX)) as u32;
        if self.min_rtt_us != 0 && sample < self.min_rtt_us {
            // Retransmit ACKed sooner than the minimum RTT: the retransmit was spurious.
            return None;
        }
        self.rack_tmit_us = entry.last_xmit_us();
        self.rack_rtt_us = sample;
        Some(sample)
    }

    /// FreeBSD `rack_note_dsack`: account a DSACK and detect a false recovery.
    pub fn note_dsack(
        &mut self,
        start: u32,
        end: u32,
        ctx: &RackContext,
        outcome: &mut RackOutcome,
    ) {
        let amount = if seq_gt(end, start) {
            end.wrapping_sub(start)
        } else {
            0
        };
        if let Some((t_start, t_end)) = self.last_tlp_acked
            && seq_ge(start, t_start)
            && seq_le(end, t_end)
        {
            self.add_dsack_bytes(amount, outcome);
            return;
        }
        if let Some((t_start, t_end)) = self.last_sent_tlp
            && seq_ge(start, t_start)
            && seq_le(end, t_end)
        {
            self.add_dsack_bytes(amount, outcome);
            return;
        }
        if !self.dsack_round_seen {
            self.dsack_round_seen = true;
            self.dsack_round_end = self.snd_max;
            self.num_dsack = self.num_dsack.saturating_add(1);
            self.dsack_persist = 16;
        }
        self.add_dsack_bytes(amount, outcome);
        self.on_dsack(ctx.srtt_ms);
        if !self.in_recovery
            && self.retran_during_recovery > 0
            && self.dsack_byte_cnt >= self.retran_during_recovery
        {
            self.might_revert = true;
            outcome.spurious = true;
            self.retran_during_recovery = 0;
            self.dsack_byte_cnt = 0;
        }
    }

    /// FreeBSD `rack_strike_dupack`: add one duplicate-ACK strike to the oldest hole.
    pub fn strike_dupack(&mut self) -> Option<(u32, u32)> {
        let index = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                !entry.sacked && entry.dupack < RACK_DUP_THRESHOLD && !entry.must_rxt
            })
            .min_by_key(|(_, entry)| entry.last_xmit_us())
            .map(|(index, _)| index)?;
        if self.entries[index].dupack < u8::MAX {
            self.entries[index].dupack = self.entries[index].dupack.saturating_add(1);
        }
        if self.entries[index].dupack >= RACK_DUP_THRESHOLD {
            Some((self.entries[index].start, self.entries[index].end))
        } else {
            None
        }
    }

    /// FreeBSD `tcp_rack_output`: pick the next retransmission or `None`.
    pub fn next_retransmit(&mut self, ctx: &RackContext) -> Option<(u32, u32)> {
        let now_us = ctx.event_us().max(1);
        let thresh = u64::from(self.reorder_threshold_us(ctx, now_us));
        let index = self.oldest_unacked_index()?;
        let entry = self.entries[index];
        if entry.must_rxt {
            return Some((entry.start, entry.end));
        }
        if entry.sacked {
            return None;
        }
        let eligible = entry.sack_passed
            || u32::from(entry.dupack) >= ctx.dup_thresh.max(u32::from(RACK_DUP_THRESHOLD));
        if !eligible && ctx.sack_permitted {
            return None;
        }
        if !ctx.sack_permitted && u32::from(entry.dupack) < RACK_DUP_THRESHOLD.into() {
            return None;
        }
        let last = entry.last_xmit_us().max(1);
        if now_us <= last || now_us.saturating_sub(last) < thresh {
            return None;
        }
        Some((entry.start, entry.end))
    }

    /// FreeBSD `rack_find_high_nonack`/TLP rescue: highest unacknowledged range.
    #[must_use]
    pub fn highest_unacked(&self) -> Option<(u32, u32)> {
        self.entries
            .iter()
            .rev()
            .find(|entry| !entry.sacked)
            .map(|entry| (entry.start, entry.end))
    }

    /// Arm the RACK tail-loss probe (`rack_calc_thresh_tlp` with the default threshold).
    pub fn tlp_arm(&mut self, ctx: &RackContext) {
        if self.entries.iter().all(|entry| entry.sacked) || self.snd_una == self.snd_max {
            self.tlp_deadline_us = None;
            return;
        }
        let srtt = u64::from(ctx.srtt_us().max(1));
        self.tlp_deadline_us = Some(ctx.event_us().saturating_add(srtt.saturating_mul(2)));
    }

    /// Fire the tail-loss probe. Returns the range to retransmit, or `None`.
    pub fn tlp_fire(&mut self, ctx: &RackContext) -> Option<(u32, u32)> {
        let deadline = self.tlp_deadline_us?;
        if ctx.event_us() < deadline || self.tlp_count >= RACK_TLP_LIMIT {
            return None;
        }
        let range = self.highest_unacked()?;
        self.tlp_count = self.tlp_count.saturating_add(1);
        self.last_sent_tlp = Some(range);
        self.update_entry(range.0, range.1.wrapping_sub(range.0), ctx.now_ms);
        if let Some(index) = self.index_for(range.0) {
            self.entries[index].tlp = true;
        }
        let srtt = u64::from(ctx.srtt_us().max(1));
        self.tlp_deadline_us = Some(ctx.event_us().saturating_add(srtt.saturating_mul(2)));
        Some(range)
    }

    /// FreeBSD `rack_cong_signal`: recovery entry/exit for the transport.
    pub fn cong_signal(&mut self, ctx: &RackContext, signal: RackSignal) -> RackCongOutcome {
        let mut result = RackCongOutcome {
            cwnd: ctx.cwnd,
            ssthresh: ctx.ssthresh,
            in_recovery: self.in_recovery,
            recover: self.recover,
            prr_sndcnt: self.prr_sndcnt,
            send_cwr: false,
        };
        match signal {
            RackSignal::DupAck => {
                if !self.in_recovery {
                    self.enter_recovery(ctx);
                    result.in_recovery = true;
                    result.recover = self.recover;
                    result.prr_sndcnt = self.prr_sndcnt;
                }
                result.send_cwr = true;
            }
            RackSignal::Ecn => {
                self.recover = self.snd_max.wrapping_add(1);
                result.send_cwr = true;
                result.recover = self.recover;
            }
            RackSignal::Rto => {
                self.undo = Some((
                    self.in_recovery,
                    self.recover,
                    self.recover_fs,
                    self.prr_delivered,
                    self.prr_out,
                ));
                self.exit_recovery();
                self.prr_sndcnt = 0;
                self.rto_ssthresh = ctx.ssthresh;
                result.cwnd = ctx.mss.max(1);
                result.ssthresh = ctx.ssthresh;
                result.in_recovery = false;
                result.send_cwr = true;
                self.tlp_deadline_us = None;
                self.tlp_count = 0;
            }
            RackSignal::RtoErr => {
                if let Some((in_recovery, recover, recover_fs, delivered, out)) = self.undo.take() {
                    self.in_recovery = in_recovery;
                    self.recover = recover;
                    self.recover_fs = recover_fs;
                    self.prr_delivered = delivered;
                    self.prr_out = out;
                    result.in_recovery = in_recovery;
                    result.recover = recover;
                }
            }
        }
        result
    }

    /// FreeBSD `do_rack_compute_pipe`.
    #[must_use]
    pub fn compute_pipe(&self, snd_una: u32) -> u32 {
        self.snd_max
            .wrapping_sub(snd_una)
            .saturating_sub(self.sacked_bytes.saturating_add(self.considered_lost))
            .saturating_add(self.holes_rxt)
    }

    /// FreeBSD `rack_update_prr`: update the PRR send allowance.
    pub fn update_prr(&mut self, changed: u32, ctx: &RackContext) -> u32 {
        if !self.in_recovery {
            return 0;
        }
        self.prr_delivered = self.prr_delivered.saturating_add(changed);
        let pipe = self.compute_pipe(ctx.ack);
        let sndcnt = if pipe > ctx.ssthresh {
            if self.recover_fs == 0 {
                0
            } else {
                (u64::from(self.prr_delivered) * u64::from(ctx.ssthresh)
                    / u64::from(self.recover_fs))
                .saturating_add(1)
                .saturating_sub(u64::from(self.prr_out)) as u32
            }
        } else {
            self.prr_delivered
                .saturating_add(2u32.saturating_mul(ctx.mss.max(1)))
                .saturating_sub(self.prr_out)
        };
        self.prr_sndcnt = sndcnt;
        self.prr_sndcnt
    }

    fn update_context_tuning(&mut self, ctx: &RackContext) {
        if ctx.reorder_fade_us != 0 {
            self.reorder_fade_us = ctx.reorder_fade_us;
        }
        if ctx.reorder_shift != 0 {
            self.reorder_shift = ctx.reorder_shift;
        }
        if ctx.pkt_delay_us != 0 {
            self.pkt_delay_us = ctx.pkt_delay_us;
        }
        if ctx.std_based {
            self.std_based = true;
        }
        if ctx.use_dsack {
            self.use_dsack = true;
        }
    }

    fn latch_rtt(&mut self, sample: u32, xmit_us: u64) {
        if self.min_rtt_us == 0 || sample < self.min_rtt_us {
            self.min_rtt_us = sample.max(1);
            self.min_rtt_ms = (sample / 1_000).max(1);
        }
        self.rack_tmit_us = xmit_us;
        self.rack_rtt_us = sample;
    }

    fn add_dsack_bytes(&mut self, amount: u32, outcome: &mut RackOutcome) {
        if amount == 0 {
            return;
        }
        self.dsack_byte_cnt = self.dsack_byte_cnt.saturating_add(amount);
        outcome.dsack = true;
    }

    fn index_for(&self, seq: u32) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| seq_ge(seq, entry.start) && seq_lt(seq, entry.end))
    }

    fn oldest_unacked_index(&self) -> Option<usize> {
        let mut best: Option<usize> = None;
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.sacked {
                continue;
            }
            if best
                .is_none_or(|current| entry.last_xmit_us() < self.entries[current].last_xmit_us())
            {
                best = Some(index);
            }
        }
        best
    }

    fn split_at_seq(&mut self, seq: u32) -> Option<usize> {
        let index = self.index_for(seq)?;
        if seq == self.entries[index].start {
            return Some(index);
        }
        let mut tail = self.entries[index];
        tail.start = seq;
        self.entries[index].end = seq;
        self.entries.insert(index + 1, tail);
        Some(index + 1)
    }

    fn process_to_cumack(
        &mut self,
        ctx: &RackContext,
        ack: u32,
        now_us: u64,
        outcome: &mut RackOutcome,
    ) {
        let index = 0;
        while index < self.entries.len() {
            let entry = self.entries[index];
            if seq_ge(ack, entry.end) {
                if !entry.sacked
                    && let Some(sample) = self.update_rtt(&entry, ctx, now_us)
                {
                    outcome.rtt_sample_us = Some(sample);
                }
                self.remove_entry(entry);
                self.entries.remove(index);
            } else if seq_gt(ack, entry.start) {
                let cut = ack.wrapping_sub(entry.start);
                self.trim_entry_head(&entry);
                if entry.sacked {
                    self.sacked_bytes = self.sacked_bytes.saturating_sub(cut);
                }
                if entry.was_lost {
                    self.considered_lost = self.considered_lost.saturating_sub(cut);
                }
                self.entries[index].start = ack;
                self.entries[index].rtr_bytes = self.entries[index].rtr_bytes.saturating_sub(cut);
                break;
            } else {
                break;
            }
        }
        self.snd_una = ack;
        if seq_gt(self.snd_una, self.snd_max) {
            self.snd_max = self.snd_una;
        }
        if !self.in_recovery && seq_gt(self.snd_una, self.recover) {
            self.recover = self.snd_una;
        }
        // FreeBSD `rack_peer_reneges`: the peer cum-ACKs the start of a previously SACKed range.
        if self
            .entries
            .first()
            .is_some_and(|entry| entry.start == ack && entry.sacked)
        {
            outcome.reneged = self.rack_peer_reneges(ack);
        }
    }

    /// FreeBSD `rack_peer_reneges`: clear contiguous SACKed ranges the peer has reneged.
    pub fn rack_peer_reneges(&mut self, ack: u32) -> u32 {
        let mut reneged = 0u32;
        for entry in &mut self.entries {
            if entry.start < ack {
                continue;
            }
            if !entry.sacked {
                break;
            }
            entry.sacked = false;
            entry.sack_passed = false;
            entry.was_sacked = false;
            entry.lost = false;
            reneged = reneged.saturating_add(entry.len());
        }
        self.sacked_bytes = self.sacked_bytes.saturating_sub(reneged);
        reneged
    }

    fn process_sack_block(
        &mut self,
        ctx: &RackContext,
        start: u32,
        end: u32,
        now_us: u64,
        outcome: &mut RackOutcome,
    ) {
        self.split_at_seq(start);
        self.split_at_seq(end);
        let mut sacked_xmit: Option<u64> = None;
        let mut index = 0;
        while index < self.entries.len() {
            let entry = self.entries[index];
            if seq_le(entry.end, start) {
                index += 1;
                continue;
            }
            if seq_ge(entry.start, end) {
                break;
            }
            if seq_ge(entry.start, start) && seq_le(entry.end, end) {
                if !entry.sacked {
                    self.entries[index].sacked = true;
                    self.entries[index].was_sacked = true;
                    self.entries[index].ack_arrival = now_us;
                    self.sacked_bytes = self.sacked_bytes.saturating_add(entry.len());
                    outcome.sacked = outcome.sacked.saturating_add(entry.len());
                }
                let stamp = entry.last_xmit_us();
                sacked_xmit = Some(sacked_xmit.map_or(stamp, |current| current.max(stamp)));
                if let Some(sample) = self.update_rtt(&entry, ctx, now_us) {
                    outcome.rtt_sample_us = Some(sample);
                }
            }
            index += 1;
        }
        let Some(sacked_xmit) = sacked_xmit else {
            return;
        };
        let thresh = u64::from(self.reorder_threshold_us(ctx, now_us));
        for entry in &mut self.entries {
            if entry.sacked || entry.start >= end || entry.last_xmit_us() > sacked_xmit {
                continue;
            }
            if !entry.sack_passed {
                entry.sack_passed = true;
                entry.was_sacked = false;
                if !entry.was_lost && now_us.saturating_sub(entry.last_xmit_us().max(1)) >= thresh {
                    entry.was_lost = true;
                    self.considered_lost = self.considered_lost.saturating_add(entry.len());
                }
                self.reorder_ts_us = now_us.max(1);
            }
        }
    }

    fn check_reorder_exit(&mut self, ctx: &RackContext, now_us: u64, outcome: &mut RackOutcome) {
        if !self.in_recovery {
            return;
        }
        let fractional = (ctx.srtt_us() / RACK_RTT_DIVISOR).max(1);
        let mut exit = false;
        for entry in &mut self.entries {
            if entry.rtr_count == 2 && entry.was_sacked {
                let snt_to_ack = now_us.saturating_sub(entry.last_xmit_us());
                if snt_to_ack <= u64::from(fractional) {
                    self.reorder_ts_us = now_us.max(1);
                    self.recovery_rxt_cnt = self.recovery_rxt_cnt.saturating_sub(entry.len());
                    if self.recovery_rxt_cnt == 0 {
                        exit = true;
                    }
                    break;
                }
            }
        }
        if exit {
            self.last_ssthresh = self.recover_ssthresh;
            self.exit_recovery();
            outcome.recovered = true;
            outcome.restored_ssthresh = Some(self.last_ssthresh);
        }
    }

    fn declare_losses(&mut self, ctx: &RackContext, now_us: u64, outcome: &mut RackOutcome) {
        let thresh = u64::from(self.reorder_threshold_us(ctx, now_us));
        let dup_thresh = ctx.dup_thresh.max(u32::from(RACK_DUP_THRESHOLD));
        let mut lost = Vec::new();
        for entry in &mut self.entries {
            if entry.sacked || entry.lost {
                continue;
            }
            let eligible =
                entry.must_rxt || entry.sack_passed || u32::from(entry.dupack) >= dup_thresh;
            if !eligible {
                continue;
            }
            let last = entry.last_xmit_us().max(1);
            if now_us <= last || now_us.saturating_sub(last) < thresh {
                continue;
            }
            entry.lost = true;
            entry.was_lost = true;
            self.considered_lost = self.considered_lost.saturating_add(entry.len());
            lost.push(entry.start);
        }
        if lost.is_empty() {
            return;
        }
        outcome.lost = lost;
        outcome.retransmit = true;
        outcome.retransmit_range = self.next_retransmit(ctx);
        if !self.in_recovery {
            self.enter_recovery(ctx);
            outcome.entered_recovery = true;
        }
    }

    fn enter_recovery(&mut self, ctx: &RackContext) {
        self.in_recovery = true;
        self.recover = self
            .snd_max
            .max(self.end_seq)
            .max(ctx.snd_max)
            .max(ctx.snd_nxt);
        self.recover_fs = self.recover.saturating_sub(self.snd_una.max(ctx.ack));
        self.prr_delivered = 0;
        self.prr_out = 0;
        self.prr_sndcnt = ctx.mss.max(1);
        self.recovery_rxt_cnt = 0;
        self.retran_during_recovery = 0;
        self.dsack_byte_cnt = 0;
        self.recover_ssthresh = ctx.ssthresh;
    }

    fn exit_recovery(&mut self) {
        self.in_recovery = false;
        self.prr_sndcnt = 0;
        self.prr_delivered = 0;
        self.prr_out = 0;
    }

    fn update_rsm(&mut self, entry: &mut RackEntry, now_us: u64, now_ms: u32) {
        if entry.rtr_count as usize >= RACK_NUM_OF_RETRANS {
            entry.rtr_count = RACK_NUM_OF_RETRANS as u8;
        } else {
            entry.rtr_count = entry.rtr_count.saturating_add(1);
        }
        let index = entry.rtr_count.max(1) as usize - 1;
        entry.dupack = 0;
        if entry.rtr_count > 1 {
            self.holes_rxt = self.holes_rxt.saturating_add(entry.len());
            entry.rtr_bytes = entry.rtr_bytes.saturating_add(entry.len());
            self.retran_during_recovery = self.retran_during_recovery.saturating_add(entry.len());
        }
        if entry.was_lost {
            entry.was_lost = false;
            entry.lost = false;
            self.considered_lost = self.considered_lost.saturating_sub(entry.len());
        }
        entry.xmit_us[index] = now_us;
        entry.xmit_ts = now_ms;
        entry.retransmitted = true;
        entry.fas = self.snd_max.wrapping_sub(self.snd_una);
        if entry.sacked {
            entry.sacked = false;
            self.sacked_bytes = self.sacked_bytes.saturating_sub(entry.len());
        }
        if entry.sack_passed {
            entry.sack_passed = false;
            entry.was_sacked = true;
            self.recovery_rxt_cnt = self.recovery_rxt_cnt.saturating_add(entry.len());
        }
        entry.must_rxt = false;
        entry.tlp = false;
    }

    fn remove_entry(&mut self, entry: RackEntry) {
        if entry.sacked {
            self.sacked_bytes = self.sacked_bytes.saturating_sub(entry.len());
        }
        if entry.was_lost || entry.lost {
            self.considered_lost = self.considered_lost.saturating_sub(entry.len());
        }
        self.holes_rxt = self
            .holes_rxt
            .saturating_sub(entry.rtr_bytes.min(entry.len()));
    }

    fn trim_entry_head(&mut self, entry: &RackEntry) {
        self.holes_rxt = self
            .holes_rxt
            .saturating_sub(entry.rtr_bytes.min(entry.len()));
    }

    /// Donor-name view of `congestion`.
    pub fn rack_cong_signal(&mut self, ctx: &RackContext, signal: RackSignal) -> RackCongOutcome {
        self.cong_signal(ctx, signal)
    }

    /// Donor-name view of `ack_received`.
    pub fn rack_ack_received(&mut self, ctx: &RackContext) -> RackOutcome {
        self.process_ack(ctx)
    }

    /// Donor-name view of `process_ack`.
    pub fn rack_process_ack(&mut self, ctx: &RackContext) -> RackOutcome {
        self.process_ack(ctx)
    }

    /// Donor-name view of `update_entry`.
    pub fn rack_update_entry(&mut self, start: u32, len: u32, now_ms: u32) {
        self.update_entry(start, len, now_ms);
    }

    /// Donor-name view of `update_rsm` for one range.
    pub fn rack_update_rsm(&mut self, start: u32, now_ms: u32) {
        if let Some(index) = self.index_for(start) {
            let mut entry = self.entries[index];
            let now_us = u64::from(now_ms).saturating_mul(1_000);
            self.update_rsm(&mut entry, now_us, now_ms);
            self.entries[index] = entry;
        }
    }

    /// Donor-name view of `update_rtt`.
    pub fn rack_update_rtt(&mut self, entry: &RackEntry, ctx: &RackContext) -> Option<u32> {
        let now_us = ctx.event_us().max(1);
        self.update_rtt(entry, ctx, now_us)
    }

    /// Donor-name view of `note_dsack`.
    pub fn rack_note_dsack(
        &mut self,
        start: u32,
        end: u32,
        ctx: &RackContext,
        outcome: &mut RackOutcome,
    ) {
        self.note_dsack(start, end, ctx, outcome);
    }

    /// Donor-name view of `calc_thresh_rack`.
    pub fn rack_calc_thresh_rack(&mut self, ctx: &RackContext) -> u32 {
        let now_us = ctx.event_us().max(1);
        self.reorder_threshold_us(ctx, now_us)
    }

    /// Donor-name view of `proc_sack_blk`.
    pub fn rack_proc_sack_blk(&mut self, ctx: &RackContext, start: u32, end: u32) -> RackOutcome {
        let now_us = ctx.event_us().max(1);
        let mut outcome = RackOutcome::default();
        self.process_sack_block(ctx, start, end, now_us, &mut outcome);
        outcome
    }

    /// FreeBSD `rack_log_sack_passed`: mark ranges older than a SACKed range.
    pub fn rack_log_sack_passed(&mut self, ctx: &RackContext, end: u32, outcome: &mut RackOutcome) {
        let now_us = ctx.event_us().max(1);
        let thresh = u64::from(self.reorder_threshold_us(ctx, now_us));
        for entry in &mut self.entries {
            if entry.sacked || entry.start >= end {
                continue;
            }
            if !entry.sack_passed {
                entry.sack_passed = true;
                entry.was_sacked = false;
                if !entry.was_lost && now_us.saturating_sub(entry.last_xmit_us().max(1)) >= thresh {
                    entry.was_lost = true;
                    self.considered_lost = self.considered_lost.saturating_add(entry.len());
                }
                self.reorder_ts_us = now_us.max(1);
            }
        }
        let _ = outcome;
    }

    /// Donor-name view of `process_to_cumack`.
    pub fn rack_process_to_cumack(&mut self, ctx: &RackContext, ack: u32) -> RackOutcome {
        let now_us = ctx.event_us().max(1);
        let mut outcome = RackOutcome::default();
        self.process_to_cumack(ctx, ack, now_us, &mut outcome);
        outcome
    }

    /// Donor-name view of `check_reorder_ack`.
    pub fn rack_check_reorder_ack(&mut self, ctx: &RackContext) -> bool {
        let now_us = ctx.event_us().max(1);
        let mut outcome = RackOutcome::default();
        self.check_reorder_exit(ctx, now_us, &mut outcome);
        outcome.recovered
    }

    /// Donor-name view of `strike_dupack`.
    pub fn rack_strike_dupack(&mut self) -> Option<(u32, u32)> {
        self.strike_dupack()
    }

    /// Donor-name view of `tcp_rack_output`.
    pub fn tcp_rack_output(&mut self, ctx: &RackContext) -> Option<(u32, u32)> {
        self.next_retransmit(ctx)
    }

    /// Donor-name view of `update_prr`.
    pub fn rack_update_prr(&mut self, changed: u32, ctx: &RackContext) -> u32 {
        self.update_prr(changed, ctx)
    }
}

/// Tail-loss-probe timing.
#[derive(Clone, Debug, Default)]
pub struct Tlp {
    /// Current probe deadline in milliseconds, if armed.
    pub deadline: Option<u32>,
}

impl Tlp {
    /// Arm the PTO for `now` when there is unacknowledged data.
    pub fn arm(&mut self, now_ms: u32, srtt_ms: u32, rttvar_ms: u32) {
        let pto = srtt_ms
            .saturating_add(4u32.saturating_mul(rttvar_ms).max(1))
            .saturating_add(2);
        self.deadline = Some(now_ms.saturating_add(pto));
    }

    /// Disarm on ACK.
    pub fn disarm(&mut self) {
        self.deadline = None;
    }

    /// TLP fires at its deadline.
    #[must_use]
    pub fn fires(&self, now_ms: u32) -> bool {
        self.deadline.is_some_and(|deadline| deadline <= now_ms)
    }

    /// Whether the TLP is sooner than an RTO deadline.
    #[must_use]
    pub fn is_sooner_than(&self, rto_deadline: Option<u32>) -> bool {
        match (self.deadline, rto_deadline) {
            (Some(tlp), Some(rto)) => tlp < rto,
            (Some(_), None) => true,
            _ => false,
        }
    }
}

/// Explicit-congestion-notification state.
#[derive(Clone, Debug, Default)]
pub struct Ecn {
    /// Both endpoints offered ECN.
    pub negotiated: bool,
    /// A CE mark has been observed in this RTT.
    pub ce_seen: bool,
    /// The congestion response has already run in this RTT.
    pub reacted: bool,
    /// ECE should be set on outgoing ACKs.
    pub ece_pending: bool,
    /// A CWR should be sent.
    pub cwr_pending: bool,
}

impl Ecn {
    /// Negotiate from the two SYN offer flags.
    pub fn negotiate(&mut self, local_offered: bool, peer_offered: bool) {
        self.negotiated = local_offered && peer_offered;
    }

    /// Observe a CE-marked packet; returns whether a reaction is due now.
    pub fn on_ce(&mut self) -> bool {
        if !self.negotiated {
            return false;
        }
        self.ce_seen = true;
        self.ece_pending = true;
        if !self.reacted {
            self.reacted = true;
            self.cwr_pending = true;
            true
        } else {
            false
        }
    }

    /// The beginning of a new RTT clears the once-per-RTT latch.
    pub fn on_rtt(&mut self) {
        self.reacted = false;
        self.ce_seen = false;
    }

    /// Consume the pending CWR.
    pub fn take_cwr(&mut self) -> bool {
        let value = self.cwr_pending;
        self.cwr_pending = false;
        value
    }
}

/// Saved congestion state for undoing a spurious retransmission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpuriousUndo {
    /// Congestion window before the loss response.
    pub cwnd: u32,
    /// Slow-start threshold before the loss response.
    pub ssthresh: u32,
    /// Whether an undo is available.
    pub armed: bool,
}

impl SpuriousUndo {
    /// Record the pre-loss state.
    #[must_use]
    pub const fn record(cwnd: u32, ssthresh: u32) -> Self {
        Self {
            cwnd,
            ssthresh,
            armed: true,
        }
    }

    /// Restore the pre-loss state, returning `(cwnd, ssthresh)` once.
    pub fn restore(&mut self) -> Option<(u32, u32)> {
        if !self.armed {
            return None;
        }
        self.armed = false;
        Some((self.cwnd, self.ssthresh))
    }
}

/// RTT-derived TLP PTO in milliseconds.
#[must_use]
pub fn pto(srtt_ms: u32, rttvar_ms: u32) -> Duration {
    Duration::from_millis(u64::from(
        srtt_ms
            .saturating_add(4u32.saturating_mul(rttvar_ms).max(1))
            .saturating_add(2),
    ))
}

#[cfg(test)]
#[path = "recovery_ut.rs"]
mod tests;
