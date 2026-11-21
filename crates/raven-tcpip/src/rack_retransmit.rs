// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 2016-2020 Netflix, Inc.
//
// Portions derived from FreeBSD `sys/netinet/tcp_stacks/rack.c` and
// `sys/netinet/tcp_stacks/tcp_rack.h` at
// 133344405f532bd54ff3629a77d4c4e062f01fdb (BSD-2-Clause). This sans-IO
// model is distributed under the project BSD-3-Clause licence; the donor
// BSD-2 notice is retained verbatim below.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! RACK ACK processing, send-map update and retransmit/rescue selection (K331).
//!
//! This is a literal Rust port of the FreeBSD `rack.c` ACK/send-map group. It keeps the donor
//! function names and operates on the merged K313 [`RackHpts`], K314 [`RackStack`] and K318
//! send-map/transport contexts. The donor mbuf and `ip_output` calls become a sans-IO
//! [`RackTransmission`] request; no packet is built here.
//!
//! Donor: `sys/netinet/tcp_stacks/rack.c` at `133344405f` (BSD-2-Clause) and
//! `sys/netinet/tcp_stacks/tcp_rack.h` for the send-map flags.

use alloc::vec::Vec;

use crate::rack_hpts::{PACE_TMR_RXT, RackHpts};
use crate::rack_types::{
    PACE_MAX_IP_BYTES, RACK_ACKED, RACK_HAD_PUSH, RACK_HAS_FIN, RACK_HAS_SYN, RACK_MUST_RXT,
    RACK_NUM_OF_RETRANS, RACK_OVERMAX, RACK_RTO_MAX, RACK_RTO_MIN, RACK_RWND_COLLAPSED,
    RACK_SACK_PASSED, RACK_SENT_FP, RACK_TLP, RACK_WAS_ACKED, RACK_WAS_LOST, RACK_WAS_SACKPASS,
    RackSendmap, TF_CONGRECOVERY, TF_FASTRECOVERY,
};
use crate::rack_vtable::{
    RackStack, TCPS_CLOSE_WAIT, TCPS_ESTABLISHED, TCPS_SYN_RECEIVED, TCPS_SYN_SENT, TF_SENTFIN,
    TH_ACK, TH_FIN, TH_RST,
};

/// `TCPS_FIN_WAIT_1`; K318 only exports the states it dispatches.
const TCPS_FIN_WAIT_1: u8 = 6;
/// `TCPS_FIN_WAIT_2`.
const TCPS_FIN_WAIT_2: u8 = 7;
/// `TCPS_CLOSING`.
const TCPS_CLOSING: u8 = 9;
/// `TCPS_LAST_ACK`.
const TCPS_LAST_ACK: u8 = 10;
/// The TCP SYN flag; `rack_vtable` does not re-export it.
const TH_SYN: u16 = 0x02;

/// `DUP_ACK_THRESHOLD`, the duplicate-ACK threshold.
pub const DUP_ACK_THRESHOLD: u8 = 3;
/// `MHLEN`, the mbuf header length used by the fast-output size guard.
pub const MHLEN: u32 = 224;
/// `max_linkhdr`, the link-layer header space used by the fast-output size guard.
pub const MAX_LINKHDR: u32 = 14;
/// `CUM_ACKED`, a cumulative-ACK RTT sample.
pub const CUM_ACKED: u8 = 1;
/// `SACKED`, a SACK RTT sample.
pub const SACKED: u8 = 2;
/// The TCP PUSH flag.
pub const TH_PUSH: u16 = 0x08;
/// `TF_GPUTINPROG`: a goodput measurement is in progress.
pub const TF_GPUTINPROG: u32 = 0x0000_8000;
/// `RACK_USE_BEG`: the measurement starts at the left edge of the send-map entry.
pub const RACK_USE_BEG: u8 = 1;
/// `RACK_USE_END`: the measurement starts at the right edge.
pub const RACK_USE_END: u8 = 2;
/// `RACK_USE_END_OR_THACK`: the measurement starts at the ACK or the right edge.
pub const RACK_USE_END_OR_THACK: u8 = 3;
/// `MIN_GP_WIN`: a goodput measurement needs at least six MSS.
pub const MIN_GP_WIN: u32 = 6;
/// `rack_rtt_divisor` default.
pub const RACK_RTT_DIVISOR: u32 = 2;

/// The sequence-ordered send map with the donor time-order list.
#[derive(Clone, Debug, Default)]
pub struct RackSendMap {
    /// Sequence-ordered entries.
    pub segments: Vec<RackSendmap>,
    /// Time-order entry identifiers (`rc_tmap`).
    pub tmap: Vec<u32>,
    next_id: u32,
}

impl RackSendMap {
    /// Insert a new transmitted segment and append it to the time-order list.
    pub fn insert(&mut self, start: u32, end: u32, ts: u64) -> u32 {
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        let index = self.segments.partition_point(|entry| entry.start < start);
        self.segments
            .insert(index, RackSendmap::with_id(id, start, end, ts));
        self.tmap.push(id);
        id
    }

    /// Find an entry by sequence start.
    #[must_use]
    pub fn find(&self, start: u32) -> Option<&RackSendmap> {
        self.segments.iter().find(|entry| entry.start == start)
    }

    /// Find an entry by identifier.
    #[must_use]
    pub fn find_id(&self, id: u32) -> Option<&RackSendmap> {
        self.segments.iter().find(|entry| entry.id == id)
    }

    /// Mutably find an entry by identifier.
    pub fn find_id_mut(&mut self, id: u32) -> Option<&mut RackSendmap> {
        self.segments.iter_mut().find(|entry| entry.id == id)
    }

    /// Find the entry covering `seq`.
    #[must_use]
    pub fn covering(&self, seq: u32) -> Option<&RackSendmap> {
        self.segments
            .iter()
            .find(|entry| seq_ge(seq, entry.start) && seq_lt(seq, entry.end))
    }

    /// Remove an entry from both orders.
    pub fn remove(&mut self, id: u32) {
        self.segments.retain(|entry| entry.id != id);
        self.tmap.retain(|entry| *entry != id);
    }

    /// Move an entry to the tail of the time-order list.
    pub fn touch(&mut self, id: u32) {
        self.tmap.retain(|entry| *entry != id);
        self.tmap.push(id);
    }

    /// Mark an entry SACKed and count its bytes.
    pub fn mark_acked(&mut self, id: u32, sacked: &mut u32) -> bool {
        let Some(entry) = self.find_id_mut(id) else {
            return false;
        };
        if entry.flags & RACK_ACKED != 0 {
            return false;
        }
        entry.flags |= RACK_ACKED;
        *sacked = sacked.saturating_add(entry.len());
        true
    }

    /// Remove entries covered by a cumulative ACK and trim a partial head.
    pub fn ack_through(&mut self, ack: u32, sacked: &mut u32) {
        let mut removed = Vec::new();
        for entry in &mut self.segments {
            if seq_le(entry.end, ack) {
                if entry.flags & RACK_ACKED != 0 {
                    *sacked = sacked.saturating_sub(entry.len());
                }
                removed.push(entry.id);
            } else if seq_lt(entry.start, ack) {
                if entry.flags & RACK_ACKED != 0 {
                    *sacked = sacked.saturating_sub(ack.wrapping_sub(entry.start));
                }
                entry.start = ack;
            }
        }
        self.segments.retain(|entry| !removed.contains(&entry.id));
        self.tmap.retain(|id| !removed.contains(id));
    }

    /// Copy the donor `rack_clone_rsm` state into an existing tail entry.
    fn clone_tail(&mut self, tail_id: u32, source_id: u32) {
        let Some(source) = self.find_id(source_id).cloned() else {
            return;
        };
        let Some(tail) = self.find_id_mut(tail_id) else {
            return;
        };
        tail.rtr_cnt = source.rtr_cnt;
        tail.act_rxt_cnt = source.act_rxt_cnt;
        tail.flags = source.flags;
        tail.dupack = source.dupack;
        tail.no_rtt_allowed = source.no_rtt_allowed;
        tail.hw_tls = source.hw_tls;
        tail.rtr_bytes = 0;
        tail.fas = source.fas;
        tail.bas = source.bas;
        tail.tim_lastsent = source.tim_lastsent;
        tail.flags &= !RACK_HAS_SYN; // RACK_HAS_SYN stays on the left edge.
    }

    /// Clear the right-edge flags after a split.
    fn clear_edge_flags(&mut self, id: u32) {
        if let Some(entry) = self.find_id_mut(id) {
            entry.flags &= !(RACK_HAS_FIN | RACK_HAD_PUSH);
        }
    }
}

/// The result of [`RackRetransmit::rack_process_ack`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessAck {
    /// ACK behind `snd_una`.
    Old,
    /// ACK above `snd_max`; the donor sends a challenge ACK.
    AboveMax,
    /// Duplicate ACK; the donor may have selected a retransmit.
    Duplicate {
        /// The entry selected by `tcp_rack_output`, if any.
        selected: Option<u32>,
    },
    /// The ACK advanced `snd_una`.
    Acked(AckOutcome),
    /// No sequence space was acknowledged.
    NoProgress,
}

/// Exact values produced by [`RackRetransmit::rack_ack_received`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AckOutcome {
    /// Bytes acknowledged.
    pub acked: u32,
    /// `t_bytes_this_ack` after the early-recovery clamp.
    pub bytes_this_ack: u32,
    /// `CCF_CWND_LIMITED`.
    pub cwnd_limited: bool,
    /// `CCF_ABC_SENTAWND`.
    pub abc_sentawnd: bool,
    /// `snd_cwnd` before the congestion-control callback.
    pub prior_cwnd: u32,
    /// The ACK exited recovery.
    pub post_recovery: bool,
}

/// The result of [`RackRetransmit::rack_update_entry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryResult {
    /// Next sequence to update, or 0 when the length was consumed.
    pub seq_out: u32,
    /// Length left after the split or overhang.
    pub remaining: u32,
    /// The new tail entry after a partial retransmit.
    pub split: Option<u32>,
}

/// Inputs for the donor `rack_fastack` fast path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FastAck<'a> {
    /// `th_ack`.
    pub ack: u32,
    /// `tiwin`.
    pub window: u32,
    /// `tlen`.
    pub tlen: u32,
    /// SACK blocks.
    pub sacks: &'a [(u32, u32)],
    /// Bytes available in the send socket buffer.
    pub sendbuf_avail: u32,
    /// Receive timestamp, µs.
    pub now_us: u64,
    /// The timestamp option was present and not behind `ts_recent`.
    pub timestamp_ok: bool,
}

/// One sans-IO fast-path retransmission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RackTransmission {
    /// `th_seq`.
    pub seq: u32,
    /// Payload length.
    pub len: u32,
    /// TCP flags without SYN/RST/FIN.
    pub flags: u16,
    /// TSO is used.
    pub tso: bool,
    /// TSO segment size.
    pub segsiz: u32,
    /// This is a TLP probe.
    pub tlp: bool,
    /// Send-map entries updated by the transmission.
    pub map_updates: u32,
}

/// A fast-path retransmission that the donor refuses to build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetransmitError {
    /// The send-map entry does not exist.
    NoSegment,
    /// SYN, RST, or a FIN-bearing entry cannot use the fast path.
    Unsendable,
    /// The computed payload is too small for an mbuf header.
    TooShort,
}

/// RACK ACK/send-map/retransmit context.
pub struct RackRetransmit {
    /// K318 transport stack carrying the K314 `RackOs` and `RackTcpcb`.
    pub stack: RackStack,
    /// K313 HPTS/timer context for the connection id.
    pub hpts: RackHpts,
    /// Connection/HPTS id.
    pub id: u32,
    /// The RACK send map.
    pub map: RackSendMap,
    /// SACKed bytes (`rc_sacked`).
    pub sacked: u32,
    /// Bytes considered lost (`rc_considered_lost`).
    pub considered_lost: u32,
    /// Retransmitted hole bytes (`rc_holes_rxt`).
    pub holes_rxt: u32,
    /// Bytes retransmitted during recovery.
    pub retran_during_recovery: u32,
    /// DSACK bytes in the current recovery.
    pub dsack_byte_cnt: u32,
    /// Bytes retransmitted on SACK passes.
    pub recovery_rxt_cnt: u32,
    /// Bytes outstanding at the RTO.
    pub out_at_rto: u32,
    /// `snd_max` at the RTO.
    pub snd_max_at_rto: u32,
    /// A retransmit timeout requires entries to be resent.
    pub must_retran: bool,
    /// Peer receive window (`snd_wnd`).
    pub snd_wnd: u32,
    /// Largest peer receive window.
    pub max_sndwnd: u32,
    /// Highest recent peer window.
    pub high_rwnd: u32,
    /// The recovery point (`snd_recover`).
    pub snd_recover: u32,
    /// Whether fast recovery is active.
    pub in_fast_recovery: bool,
    /// Whether congestion recovery is active.
    pub in_cong_recovery: bool,
    /// Whether the connection is in persist.
    pub in_persist: bool,
    /// `t_dupacks`.
    pub dupacks: u16,
    /// Smoothed RTT in µs (q0 for the model).
    pub srtt: u32,
    /// RTT variation in µs.
    pub rttvar: u32,
    /// Current RTO in µs.
    pub t_rxtcur: u32,
    /// RTO minimum in µs.
    pub rto_min: u32,
    /// RTO maximum in µs.
    pub rto_max: u32,
    /// Retransmit shift.
    pub t_rxtshift: u8,
    /// Lowest RTT observed by the connection.
    pub t_rttlow: u32,
    /// RACK minimum RTT.
    pub rack_min_rtt: u32,
    /// RACK latest RTT.
    pub rack_rtt: u32,
    /// RACK latest transmit timestamp.
    pub rack_tmit_time: u64,
    /// Last reordering timestamp.
    pub reorder_ts: u32,
    /// Reordering fade, 0 means reordering never fades.
    pub reorder_fade: u32,
    /// Standard-based RACK timer (`srtt + srtt/4`).
    pub std_based: bool,
    /// Packet-delay term in µs.
    pub pkt_delay: u32,
    /// Reordering shift.
    pub reorder_shift: u32,
    /// DSACK count used by the threshold.
    pub num_dsack: u32,
    /// `V_tcp_abc_l_var`.
    pub abc_l_var: u32,
    /// `t_bytes_acked`.
    pub bytes_acked: u32,
    /// `rc_early_recovery_segs`.
    pub early_recovery_segs: u32,
    /// The congestion window used by ACK accounting.
    pub cwnd_to_use: u32,
    /// `rc_prr_delivered`.
    pub prr_delivered: u32,
    /// `rc_prr_out`.
    pub prr_out: u32,
    /// `rc_prr_sndcnt`.
    pub prr_sndcnt: u32,
    /// `rc_prr_recovery_fs`.
    pub prr_recovery_fs: u32,
    /// `rc_pace_min_segs`.
    pub pace_min_segs: u32,
    /// `rc_pace_max_segs`.
    pub pace_max_segs: u32,
    /// `rc_user_set_max_segs`.
    pub user_set_max_segs: u32,
    /// `rack_force_max_seg`.
    pub force_max_seg: bool,
    /// The peer permitted SACK.
    pub sack_permit: bool,
    /// `rack_no_prr`.
    pub no_prr: bool,
    /// `r_fast_output`.
    pub fast_output: bool,
    /// `r_wanted_output`.
    pub want_output: bool,
    /// `r_timer_override`.
    pub timer_override: bool,
    /// `r_must_retran` complement.
    pub full_size_rxt: bool,
    /// Shape retransmits to the pacing minimum.
    pub shape_rxt_to_pacing_min: bool,
    /// `t_rtttime`.
    pub t_rtttime: u32,
    /// `t_rtseq`.
    pub t_rtseq: u32,
    /// TLP probe in progress.
    pub tlp_in_progress: bool,
    /// TLP probes sent.
    pub tlp_cnt_out: u32,
    /// `rack_fto_rsm_send`.
    pub fto_rsm_send: u64,
    /// `tcps_sndrexmitpack`.
    pub sndrexmitpack: u64,
    /// `tcps_sndrexmitbyte`.
    pub sndrexmitbyte: u64,
    /// `rack_tlp_retran`.
    pub tlp_retran: u64,
    /// `rack_tlp_retran_bytes`.
    pub tlp_retran_bytes: u64,
    /// `tp->gput_seq`.
    pub gput_seq: u32,
    /// `tp->gput_ts`.
    pub gput_ts: u32,
    /// `tp->gput_ack`.
    pub gput_ack: u32,
    /// `rack->app_limited_needs_set`.
    pub app_limited_needs_set: bool,
    /// `rack->r_ctl.rc_gp_output_ts`.
    pub gp_output_ts: u32,
    /// `rack->rc_gp_filled`.
    pub gp_filled: bool,
    /// `rack->in_probe_rtt`.
    pub in_probe_rtt: bool,
    /// `rack->measure_saw_probe_rtt`.
    pub measure_saw_probe_rtt: bool,
    /// `rack->r_ctl.rc_probertt_sndmax_atexit`.
    pub probertt_sndmax_atexit: u32,
    /// `rack->r_ctl.rc_app_limited_cnt`.
    pub app_limited_cnt: u32,
    /// `rack_get_measure_window` result used by the filled measurement.
    pub measure_window: u32,
    /// `rc_init_window` result for the current MSS.
    pub initial_window: u32,
    /// `rack->r_ctl.act_rcv_time` in microseconds.
    pub act_rcv_time: u32,
    /// `rack_rtt_divisor`.
    pub rtt_divisor: u32,
    /// `rack->r_ctl.rc_ssthresh_at_erec`.
    pub ssthresh_at_erec: u32,
    /// `rack->r_might_revert`.
    pub might_revert: bool,
    /// Number of `rack_tend_gp_marks` calls made by `rack_need_set_test`.
    pub gp_marks_tended: u32,
    /// Number of `rack_log_gpset` calls made by `rack_need_set_test`.
    pub gp_logs: u32,
    /// Number of `rack_log_to_prr` calls made by `rack_check_reorder_ack`.
    pub prr_logs: u32,
}

impl Default for RackRetransmit {
    fn default() -> Self {
        Self::new(1_460)
    }
}

impl RackRetransmit {
    /// Create a context in the established state with one MSS in flight.
    #[must_use]
    pub fn new(mss: u16) -> Self {
        let mut stack = RackStack::new();
        stack.tp.t_state = TCPS_ESTABLISHED;
        stack.tp.t_maxseg = mss;
        stack.tp.snd_cwnd = u32::from(mss).saturating_mul(10);
        stack.tp.snd_ssthresh = u32::MAX;
        stack.tp.snd_una = 1;
        stack.tp.snd_nxt = 1;
        stack.tp.snd_max = 1;
        let mut hpts = RackHpts::new(0);
        hpts.tcp_hpts_init(1);
        let pace_min_segs = u32::from(mss);
        let initial_window = rc_init_window(u32::from(mss));
        Self {
            stack,
            hpts,
            id: 1,
            map: RackSendMap::default(),
            sacked: 0,
            considered_lost: 0,
            holes_rxt: 0,
            retran_during_recovery: 0,
            dsack_byte_cnt: 0,
            recovery_rxt_cnt: 0,
            out_at_rto: 0,
            snd_max_at_rto: 0,
            must_retran: false,
            snd_wnd: 65_535,
            max_sndwnd: 65_535,
            high_rwnd: 65_535,
            snd_recover: 1,
            in_fast_recovery: false,
            in_cong_recovery: false,
            in_persist: false,
            dupacks: 0,
            srtt: 0,
            rttvar: 0,
            t_rxtcur: RACK_RTO_MIN,
            rto_min: RACK_RTO_MIN,
            rto_max: RACK_RTO_MAX,
            t_rxtshift: 0,
            t_rttlow: 0,
            rack_min_rtt: 0,
            rack_rtt: 0,
            rack_tmit_time: 0,
            reorder_ts: 0,
            reorder_fade: 0,
            std_based: false,
            pkt_delay: 0,
            reorder_shift: 0,
            num_dsack: 0,
            abc_l_var: 2,
            bytes_acked: 0,
            early_recovery_segs: 4,
            cwnd_to_use: u32::from(mss).saturating_mul(10),
            prr_delivered: 0,
            prr_out: 0,
            prr_sndcnt: 0,
            prr_recovery_fs: 0,
            pace_min_segs,
            pace_max_segs: 0,
            user_set_max_segs: 0,
            force_max_seg: false,
            sack_permit: false,
            no_prr: false,
            fast_output: false,
            want_output: false,
            timer_override: false,
            full_size_rxt: false,
            shape_rxt_to_pacing_min: false,
            t_rtttime: 0,
            t_rtseq: 0,
            tlp_in_progress: false,
            tlp_cnt_out: 0,
            fto_rsm_send: 0,
            sndrexmitpack: 0,
            sndrexmitbyte: 0,
            tlp_retran: 0,
            tlp_retran_bytes: 0,
            gput_seq: 0,
            gput_ts: 0,
            gput_ack: 0,
            app_limited_needs_set: false,
            gp_output_ts: 0,
            gp_filled: false,
            in_probe_rtt: false,
            measure_saw_probe_rtt: false,
            probertt_sndmax_atexit: 0,
            app_limited_cnt: 0,
            measure_window: initial_window,
            initial_window,
            act_rcv_time: 0,
            rtt_divisor: RACK_RTT_DIVISOR,
            ssthresh_at_erec: 0,
            might_revert: false,
            gp_marks_tended: 0,
            gp_logs: 0,
            prr_logs: 0,
        }
    }

    /// Record a new transmission.
    pub fn send(&mut self, start: u32, end: u32, now_us: u64) -> u32 {
        let id = self.map.insert(start, end, now_us);
        let flight = self.flight_size();
        if let Some(entry) = self.map.find_id_mut(id) {
            entry.fas = flight;
        }
        if seq_gt(end, self.stack.tp.snd_max) {
            self.stack.tp.snd_max = end;
        }
        if seq_lt(self.stack.tp.snd_nxt, self.stack.tp.snd_max) {
            self.stack.tp.snd_nxt = self.stack.tp.snd_max;
        }
        id
    }

    /// Reset the model at a sequence base.
    pub fn set_base(&mut self, snd_una: u32) {
        self.stack.tp.snd_una = snd_una;
        self.stack.tp.snd_nxt = snd_una;
        self.stack.tp.snd_max = snd_una;
        self.snd_recover = snd_una;
        self.cwnd_to_use = self.stack.tp.snd_cwnd;
    }

    /// Outstanding bytes: `snd_max - snd_una`.
    #[must_use]
    pub fn outstanding(&self) -> u32 {
        self.stack.tp.snd_max.wrapping_sub(self.stack.tp.snd_una)
    }

    /// Flight size: `snd_max - snd_una - sacked`.
    #[must_use]
    pub fn flight_size(&self) -> u32 {
        self.outstanding().saturating_sub(self.sacked)
    }

    /// `rack_find_lowest_rsm`: the oldest outstanding entry in time order.
    #[must_use]
    pub fn rack_find_lowest_rsm(&self) -> Option<u32> {
        self.map.tmap.iter().copied().find(|id| {
            self.map
                .find_id(*id)
                .is_some_and(|entry| entry.flags & RACK_ACKED == 0)
        })
    }

    /// `rack_find_high_nonack`: the highest sequence not ACKed or FIN, at or below `id`.
    #[must_use]
    pub fn rack_find_high_nonack(&self, id: u32) -> Option<u32> {
        let start = self.map.segments.iter().position(|entry| entry.id == id)?;
        self.map.segments[..=start]
            .iter()
            .rev()
            .find(|entry| entry.flags & (RACK_ACKED | RACK_HAS_FIN) == 0)
            .map(|entry| entry.id)
    }

    /// `rack_need_set_test`: move the goodput measurement window after an ACK.
    ///
    /// `use_which` is `RACK_USE_BEG`, `RACK_USE_END` or `RACK_USE_END_OR_THACK`.
    /// The donor logging calls become the `gp_logs`/`gp_marks_tended` counters.
    pub fn rack_need_set_test(&mut self, id: u32, th_ack: u32, use_which: u8) {
        let Some(rsm) = self.map.find_id(id).cloned() else {
            return;
        };
        if self.stack.tp.t_flags & TF_GPUTINPROG == 0 || !seq_ge(rsm.end, self.gput_seq) {
            return;
        }
        if rsm.rtr_cnt > 1 {
            // A retransmission has an ambiguous transmit timestamp.
            return;
        }
        let seq = self.gput_seq;
        let ts = self.gput_ts;
        self.app_limited_needs_set = false;
        self.gput_ts = self.act_rcv_time;
        if use_which == RACK_USE_BEG && seq_ge(rsm.start, self.gput_seq) {
            self.gput_seq = rsm.start;
        }
        if use_which == RACK_USE_END && seq_ge(rsm.end, self.gput_seq) {
            self.gput_seq = rsm.end;
        }
        if use_which == RACK_USE_END_OR_THACK {
            self.gput_seq = if seq_gt(th_ack, rsm.end) {
                th_ack
            } else {
                rsm.end
            };
        }
        let next_sent = if seq_lt(self.gput_seq, self.stack.tp.snd_max) {
            self.map
                .covering(self.gput_seq)
                .map(|entry| entry.tim_lastsent[0] as u32)
        } else {
            None
        };
        if let Some(ts) = next_sent {
            self.gp_output_ts = ts;
        } else {
            self.gp_output_ts = rsm.tim_lastsent[0] as u32;
            self.app_limited_needs_set = true;
        }
        if seq_gt(self.gput_seq, self.gput_ack) {
            let window = if self.gp_filled {
                self.measure_window
            } else {
                self.initial_window
                    .max(MIN_GP_WIN.saturating_mul(u32::from(self.stack.tp.t_maxseg)))
            };
            self.gput_ack = self.gput_seq.saturating_add(window);
        }
        if !self.in_probe_rtt
            && self.measure_saw_probe_rtt
            && seq_ge(self.gput_seq, self.probertt_sndmax_atexit)
        {
            self.measure_saw_probe_rtt = false;
        }
        self.gp_logs = self.gp_logs.saturating_add(1);
        if self.gp_filled {
            let minimum = self
                .initial_window
                .max(MIN_GP_WIN.saturating_mul(u32::from(self.stack.tp.t_maxseg)));
            if self.gput_ack.saturating_sub(self.gput_seq) < minimum {
                let ideal = self.measure_window.max(self.initial_window);
                if ideal > self.stack.os.sbavail() {
                    self.stack.tp.t_flags &= !TF_GPUTINPROG;
                    self.gp_logs = self.gp_logs.saturating_add(1);
                } else {
                    self.gput_ack = self.gput_seq.saturating_add(ideal);
                }
            }
        }
        self.gp_marks_tended = self.gp_marks_tended.saturating_add(1);
        let _ = (seq, ts);
    }

    /// `rack_check_reorder_ack`: undo a spurious recovery retransmission on a reordered ACK.
    pub fn rack_check_reorder_ack(
        &mut self,
        id: u32,
        the_end: u32,
        cts: u32,
        can_exit_recovery: bool,
    ) -> bool {
        let Some(rsm) = self.map.find_id(id).cloned() else {
            return false;
        };
        if self.rtt_divisor == 0 || rsm.rtr_cnt != 2 {
            return false;
        }
        let in_recovery = self.in_fast_recovery || self.in_cong_recovery;
        if !in_recovery || rsm.flags & RACK_WAS_SACKPASS == 0 {
            return false;
        }
        let mut fractional = self.stack.tp.t_srtt / self.rtt_divisor;
        if fractional == 0 {
            fractional = 1;
        }
        let index = usize::from(rsm.rtr_cnt.saturating_sub(1));
        let snt_to_ack = cts.wrapping_sub(rsm.tim_lastsent[index] as u32);
        if snt_to_ack > fractional {
            return false;
        }
        self.reorder_ts = cts;
        let removed = the_end.wrapping_sub(rsm.start);
        self.recovery_rxt_cnt = self.recovery_rxt_cnt.saturating_sub(removed);
        self.prr_logs = self.prr_logs.saturating_add(1);
        if can_exit_recovery && self.recovery_rxt_cnt == 0 {
            self.stack.tp.snd_ssthresh = self.ssthresh_at_erec;
            self.stack.tp.t_flags &= !(TF_FASTRECOVERY | TF_CONGRECOVERY);
            self.in_fast_recovery = false;
            self.in_cong_recovery = false;
            self.might_revert = false;
            self.retran_during_recovery = 0;
            self.prr_logs = self.prr_logs.saturating_add(1);
        }
        true
    }

    /// `rack_update_rsm`: account one retransmission of a send-map entry.
    pub fn rack_update_rsm(&mut self, id: u32, ts: u64, _add_flag: u32, segsiz: u32) {
        let flight = self.flight_size();
        let Some(entry) = self.map.find_id_mut(id) else {
            return;
        };
        entry.rtr_cnt = entry.rtr_cnt.saturating_add(1);
        if entry.rtr_cnt as usize > RACK_NUM_OF_RETRANS {
            entry.rtr_cnt = RACK_NUM_OF_RETRANS as u8;
            entry.flags |= RACK_OVERMAX;
        }
        entry.act_rxt_cnt = entry.act_rxt_cnt.saturating_add(1);
        entry.dupack = 0;
        let len = entry.len();
        let mut holes = 0;
        if entry.rtr_cnt > 1 && entry.flags & RACK_TLP == 0 {
            holes = len;
            entry.rtr_bytes = entry.rtr_bytes.saturating_add(len);
        }
        let was_lost = entry.flags & RACK_WAS_LOST != 0;
        if was_lost {
            entry.flags &= !RACK_WAS_LOST;
        }
        let index = entry.rtr_cnt as usize - 1;
        entry.tim_lastsent[index] = ts;
        entry.fas = flight;
        let was_acked = entry.flags & RACK_ACKED != 0;
        if was_acked {
            entry.flags &= !RACK_ACKED;
        }
        entry.in_tmap = true;
        let segsiz = segsiz.max(1);
        entry.bas = u8::try_from(len.div_ceil(segsiz)).unwrap_or(u8::MAX);
        let must_rxt = entry.flags & RACK_MUST_RXT != 0;
        let reaches_rto_max = seq_ge(entry.end, self.snd_max_at_rto);
        if must_rxt {
            entry.flags &= !RACK_MUST_RXT;
        }
        entry.flags &= !RACK_RWND_COLLAPSED;
        let sack_passed = entry.flags & RACK_SACK_PASSED != 0;
        if sack_passed {
            entry.flags &= !RACK_SACK_PASSED;
            entry.flags |= RACK_WAS_SACKPASS;
        }
        self.holes_rxt = self.holes_rxt.saturating_add(holes);
        if was_lost {
            self.considered_lost = self.considered_lost.saturating_sub(len);
        }
        if was_acked {
            self.sacked = self.sacked.saturating_sub(len);
        }
        if must_rxt {
            if self.must_retran {
                self.out_at_rto = self.out_at_rto.saturating_sub(len);
            }
            if reaches_rto_max {
                self.must_retran = false;
                self.out_at_rto = 0;
            }
        }
        if sack_passed {
            self.recovery_rxt_cnt = self.recovery_rxt_cnt.saturating_add(len);
        }
        self.map.touch(id);
    }

    /// `rack_update_entry`: account a (possibly partial) retransmission.
    pub fn rack_update_entry(
        &mut self,
        id: u32,
        ts: u64,
        len: u32,
        add_flag: u32,
        segsiz: u32,
    ) -> EntryResult {
        let Some(entry) = self.map.find_id(id) else {
            return EntryResult {
                seq_out: 0,
                remaining: 0,
                split: None,
            };
        };
        let start = entry.start;
        let end = entry.end;
        let c_end = start.saturating_add(len);
        if seq_ge(c_end, end) {
            self.rack_update_rsm(id, ts, add_flag, segsiz);
            if c_end == end {
                return EntryResult {
                    seq_out: 0,
                    remaining: 0,
                    split: None,
                };
            }
            let actual = end.saturating_sub(start);
            return EntryResult {
                seq_out: end,
                remaining: len.saturating_sub(actual),
                split: None,
            };
        }
        let tail = self.map.insert(c_end, end, ts);
        self.map.clone_tail(tail, id);
        if let Some(entry) = self.map.find_id_mut(id) {
            entry.end = c_end;
        }
        self.map.clear_edge_flags(id);
        self.rack_update_rsm(id, ts, add_flag, segsiz);
        EntryResult {
            seq_out: 0,
            remaining: 0,
            split: Some(tail),
        }
    }

    /// `rack_update_rtt`: sample the RTT of an ACKed or SACKed entry.
    pub fn rack_update_rtt(
        &mut self,
        id: u32,
        rtt_sample: Option<u32>,
        cts: u64,
        ack_type: u8,
        th_ack: u32,
    ) -> Option<u32> {
        let entry = self.map.find_id(id)?;
        if entry.flags & (RACK_ACKED | RACK_WAS_ACKED) != 0 || entry.no_rtt_allowed {
            return None;
        }
        let last = entry.last_sent();
        let count = entry.rtr_cnt;
        let _len_acked = if ack_type == CUM_ACKED {
            if seq_gt(th_ack, entry.end) {
                entry.len()
            } else {
                th_ack.wrapping_sub(entry.start)
            }
        } else {
            entry.len()
        };
        let t = if count == 1 {
            u32::try_from(cts.saturating_sub(last))
                .unwrap_or(u32::MAX)
                .max(1)
        } else {
            match rtt_sample {
                Some(sample) => sample.max(1),
                None => u32::try_from(cts.saturating_sub(last))
                    .unwrap_or(u32::MAX)
                    .max(1),
            }
        };
        if count > 1 && self.rack_min_rtt != 0 && t < self.rack_min_rtt {
            // A retransmitted segment acked faster than the minimum is a spurious retransmit.
            return None;
        }
        if self.t_rttlow == 0 || t < self.t_rttlow {
            self.t_rttlow = t;
        }
        if self.rack_min_rtt == 0 || t < self.rack_min_rtt {
            self.rack_min_rtt = t.max(1);
        }
        self.update_srtt(t);
        if self.rack_tmit_time == 0 || last > self.rack_tmit_time {
            self.rack_tmit_time = last.max(1);
            self.rack_rtt = t;
        }
        Some(t)
    }

    fn update_srtt(&mut self, sample: u32) {
        if self.srtt == 0 {
            self.srtt = sample.saturating_mul(8);
            self.rttvar = sample.saturating_mul(2);
        } else {
            let delta = (self.srtt >> 3).abs_diff(sample);
            self.rttvar = self.rttvar.saturating_mul(3).saturating_add(delta) >> 2;
            self.srtt = self
                .srtt
                .saturating_mul(7)
                .saturating_add(sample.saturating_mul(8))
                >> 3;
        }
        let value = (self.srtt >> 3).saturating_add(self.rttvar);
        self.t_rxtcur = value.clamp(self.rto_min, self.rto_max);
    }

    /// `rack_update_seg`: recompute the pacing minimum and maximum segment sizes.
    pub fn rack_update_seg(&mut self) -> bool {
        let original_min = self.pace_min_segs;
        let original_max = self.pace_max_segs;
        self.pace_min_segs = u32::from(self.stack.tp.t_maxseg);
        let user_max = self
            .pace_min_segs
            .saturating_mul(self.user_set_max_segs.max(1));
        if self.force_max_seg {
            self.pace_max_segs = user_max;
        }
        if self.pace_max_segs > PACE_MAX_IP_BYTES {
            self.pace_max_segs = PACE_MAX_IP_BYTES;
        }
        self.pace_min_segs != original_min || self.pace_max_segs != original_max
    }

    /// `rack_what_can_we_send`: bytes allowed by cwnd, rwnd and the send buffer.
    #[must_use]
    pub fn rack_what_can_we_send(&self, cwnd_to_use: u32, avail: u32, sb_offset: u32) -> u32 {
        let sendwin = self.snd_wnd.min(cwnd_to_use);
        if self.outstanding() >= self.snd_wnd {
            return 0;
        }
        let flight = self.flight_size();
        if flight >= sendwin {
            return 0;
        }
        let mut len = sendwin - flight;
        if len.saturating_add(self.outstanding()) > self.snd_wnd {
            len = self.snd_wnd.saturating_sub(self.outstanding());
        }
        if len.saturating_add(sb_offset) > avail {
            len = avail.saturating_sub(sb_offset);
        }
        len
    }

    /// `rack_init_retransmit_value`: select 1 MSS, pacing-min-shaped, or donor default.
    pub fn rack_init_retransmit_value(&mut self, ctl: i32) {
        match ctl {
            1 => {
                self.full_size_rxt = true;
                self.shape_rxt_to_pacing_min = false;
            }
            2 => {
                self.full_size_rxt = false;
                self.shape_rxt_to_pacing_min = true;
            }
            _ => {
                self.full_size_rxt = false;
                self.shape_rxt_to_pacing_min = false;
            }
        }
    }

    /// `rack_strike_dupack`: count a duplicate ACK and select a retransmit at the threshold.
    ///
    /// The donor calls `tcp_rack_output` here. K330 owns that output entry point; the selection
    /// it performs is reproduced by [`Self::select_resend`]. The RACK threshold comes from
    /// `rack_calc_thresh_rack` (K333), so the caller supplies it.
    pub fn rack_strike_dupack(&mut self, th_ack: u32, thresh: u32, now_us: u64) -> Option<u32> {
        let id = self.map.tmap.iter().copied().find(|id| {
            self.map.find_id(*id).is_some_and(|entry| {
                entry.dupack < DUP_ACK_THRESHOLD && entry.flags & RACK_MUST_RXT == 0
            })
        })?;
        if let Some(entry) = self.map.find_id_mut(id)
            && entry.dupack < u8::MAX
        {
            entry.dupack = entry.dupack.saturating_add(1);
        }
        let ready = self
            .map
            .find_id(id)
            .is_some_and(|entry| entry.dupack >= DUP_ACK_THRESHOLD);
        if !ready {
            return None;
        }
        let selected = self.select_resend(now_us, thresh);
        if selected.is_some() {
            if !self.in_fast_recovery {
                self.enter_fast_recovery(th_ack);
            }
            self.want_output = true;
            self.timer_override = true;
        }
        selected
    }

    fn select_resend(&mut self, now_us: u64, thresh: u32) -> Option<u32> {
        if self.map.tmap.is_empty() || self.stack.tp.t_flags & TF_SENTFIN != 0 {
            return None;
        }
        let first = *self.map.tmap.first()?;
        let mut id = first;
        if self
            .map
            .find_id(id)
            .is_some_and(|entry| entry.flags & RACK_ACKED != 0)
        {
            id = self.rack_find_lowest_rsm()?;
        }
        let entry = self.map.find_id(id)?;
        if self.must_retran && entry.flags & RACK_MUST_RXT != 0 {
            return Some(id);
        }
        if !self.sack_permit && entry.dupack >= DUP_ACK_THRESHOLD {
            return Some(id);
        }
        if entry.flags & RACK_ACKED != 0 {
            return None;
        }
        if entry.flags & RACK_SACK_PASSED == 0 && entry.dupack < DUP_ACK_THRESHOLD {
            return None;
        }
        let low = entry.last_sent();
        if now_us <= low || now_us.saturating_sub(low) < u64::from(thresh) {
            return None;
        }
        if entry.dupack >= DUP_ACK_THRESHOLD || entry.flags & RACK_SACK_PASSED != 0 {
            self.fast_output = false;
            return Some(id);
        }
        None
    }

    /// `rack_ack_received`: ABC and cwnd-limited accounting before the CC callback.
    pub fn rack_ack_received(
        &mut self,
        th_ack: u32,
        nsegs: u16,
        post_recovery: bool,
    ) -> AckOutcome {
        let acked = th_ack.wrapping_sub(self.stack.tp.snd_una);
        let mut bytes_this_ack = acked;
        if post_recovery && self.early_recovery_segs != 0 {
            let max = self
                .early_recovery_segs
                .saturating_mul(u32::from(self.stack.tp.t_maxseg));
            bytes_this_ack = bytes_this_ack.min(max);
        }
        let cwnd_limited = self.stack.tp.snd_cwnd <= self.snd_wnd;
        let mut abc_sentawnd = false;
        if self.stack.tp.snd_cwnd > self.stack.tp.snd_ssthresh {
            let add = bytes_this_ack.min(
                u32::from(nsegs.max(1))
                    .saturating_mul(self.abc_l_var)
                    .saturating_mul(u32::from(self.stack.tp.t_maxseg)),
            );
            self.bytes_acked = self.bytes_acked.saturating_add(add);
            if self.bytes_acked >= self.cwnd_to_use {
                self.bytes_acked = self.bytes_acked.saturating_sub(self.cwnd_to_use);
                abc_sentawnd = true;
            }
        } else {
            self.bytes_acked = 0;
        }
        AckOutcome {
            acked,
            bytes_this_ack,
            cwnd_limited,
            abc_sentawnd,
            prior_cwnd: self.stack.tp.snd_cwnd,
            post_recovery,
        }
    }

    /// `rack_process_ack`: the full-ACK path, recovery exit and send-map advance.
    pub fn rack_process_ack(
        &mut self,
        ack: u32,
        window: u32,
        orig_tlen: u32,
        sacks: &[(u32, u32)],
        sendbuf_avail: u32,
        now_us: u64,
    ) -> ProcessAck {
        self.stack.os.now_us = i64::try_from(now_us).unwrap_or(i64::MAX);
        self.act_rcv_time = u32::try_from(now_us).unwrap_or(u32::MAX);
        if seq_lt(ack, self.stack.tp.snd_una) {
            if self.stack.tp.snd_una.wrapping_sub(ack) > 0
                && self.reorder_ts == 0
                && sendbuf_avail > u32::from(self.stack.tp.t_maxseg)
            {
                self.reorder_ts = u32::try_from(now_us).unwrap_or(u32::MAX).max(1);
            }
            return ProcessAck::Old;
        }
        if seq_gt(ack, self.stack.tp.snd_max) {
            self.want_output = true;
            return ProcessAck::AboveMax;
        }
        if ack == self.stack.tp.snd_una
            && window == self.snd_wnd
            && orig_tlen == 0
            && sacks.is_empty()
        {
            let thresh = self.current_thresh(now_us);
            let selected = self.rack_strike_dupack(ack, thresh, now_us);
            return ProcessAck::Duplicate { selected };
        }
        if ack == self.stack.tp.snd_una {
            return ProcessAck::NoProgress;
        }
        self.t_rxtshift = 0;
        self.t_rxtcur = self.rack_rexmtval();
        self.tlp_in_progress = false;
        self.tlp_cnt_out = 0;
        if self.hpts.rack_timer_cancel(self.id) & PACE_TMR_RXT != 0 {
            self.timer_override = false;
        }
        let mut post_recovery = false;
        if self.in_fast_recovery {
            if seq_lt(ack, self.snd_recover) && seq_lt(ack, self.stack.tp.snd_max) {
                if self.prr_sndcnt > 0 || self.no_prr {
                    self.want_output = true;
                }
            } else {
                self.finish_recovery();
                post_recovery = true;
            }
        }
        let outcome = self.rack_ack_received(ack, 1, post_recovery);
        let acked_amount = outcome.acked.min(sendbuf_avail);
        self.snd_wnd = self.snd_wnd.saturating_sub(acked_amount);
        self.sacked = self.sacked.min(self.outstanding());
        self.map.ack_through(ack, &mut self.sacked);
        self.stack.tp.snd_una = ack;
        if seq_gt(self.stack.tp.snd_una, self.snd_recover) {
            self.snd_recover = self.stack.tp.snd_una;
        }
        if seq_lt(self.stack.tp.snd_nxt, self.stack.tp.snd_max) {
            self.stack.tp.snd_nxt = self.stack.tp.snd_max;
        }
        if self.stack.tp.snd_una == self.stack.tp.snd_max {
            self.stack.tp.t_flags &= !TF_SENTFIN;
            self.retran_during_recovery = 0;
            self.dsack_byte_cnt = 0;
            let _ = self.hpts.rack_timer_cancel(self.id);
            self.want_output = true;
        }
        ProcessAck::Acked(outcome)
    }

    /// `rack_fastack`: the pure-ACK fast path for one advancing ACK.
    pub fn rack_fastack(&mut self, input: FastAck<'_>) -> bool {
        let FastAck {
            ack,
            window,
            tlen,
            sacks,
            sendbuf_avail,
            now_us,
            timestamp_ok,
        } = input;
        self.act_rcv_time = u32::try_from(now_us).unwrap_or(u32::MAX);
        let _ = tlen;
        if !seq_gt(ack, self.stack.tp.snd_una)
            || seq_gt(ack, self.stack.tp.snd_max)
            || window == 0
            || !timestamp_ok
            || self.in_fast_recovery
            || self.sacked != 0
        {
            return false;
        }
        let _ = sacks;
        if window != self.snd_wnd {
            self.snd_wnd = window;
            if self.snd_wnd > self.max_sndwnd {
                self.max_sndwnd = self.snd_wnd;
            }
        }
        let persist_window = (self.high_rwnd / 2).min(self.pace_min_segs);
        if self.in_persist && self.snd_wnd >= persist_window {
            self.in_persist = false;
        } else if !self.in_persist
            && self.snd_wnd < persist_window
            && self.stack.tp.t_state >= TCPS_ESTABLISHED
            && self.stack.tp.snd_max == self.stack.tp.snd_una
            && sendbuf_avail > 0
            && sendbuf_avail > self.snd_wnd
        {
            self.in_persist = true;
        }
        let _ = self.rack_ack_received(ack, 1, false);
        let acked = ack.wrapping_sub(self.stack.tp.snd_una);
        self.sacked = self.sacked.min(self.outstanding());
        self.map.ack_through(ack, &mut self.sacked);
        self.stack.tp.snd_una = ack;
        self.t_rxtshift = 0;
        self.t_rxtcur = self.rack_rexmtval();
        self.tlp_in_progress = false;
        self.tlp_cnt_out = 0;
        let _ = self.hpts.rack_timer_cancel(self.id);
        if self.stack.tp.snd_una == self.stack.tp.snd_max {
            self.stack.tp.t_flags &= !TF_SENTFIN;
            self.retran_during_recovery = 0;
            self.dsack_byte_cnt = 0;
            let _ = self.hpts.rack_timer_cancel(self.id);
        }
        let _ = acked;
        if sendbuf_avail > 0 {
            self.want_output = true;
        }
        let _ = now_us;
        true
    }

    /// `rack_fast_rsm_output`: build one sans-IO fast-path retransmission.
    pub fn rack_fast_rsm_output(
        &mut self,
        id: u32,
        len: u32,
        doing_tlp: bool,
        now_us: u64,
        hdrlen: u32,
    ) -> Result<RackTransmission, RetransmitError> {
        if self.map.find_id(id).is_none() {
            return Err(RetransmitError::NoSegment);
        }
        if doing_tlp {
            if let Some(entry) = self.map.find_id_mut(id) {
                entry.flags |= RACK_TLP;
            }
        } else if let Some(entry) = self.map.find_id_mut(id) {
            entry.flags &= !RACK_TLP;
        }
        let entry = self.map.find_id(id).ok_or(RetransmitError::NoSegment)?;
        let mut flags = tcp_outflags(self.stack.tp.t_state);
        if flags & (TH_SYN | TH_RST) != 0 || entry.flags & RACK_HAS_FIN != 0 {
            return Err(RetransmitError::Unsendable);
        }
        flags &= !TH_FIN;
        let segsiz = u32::from(self.stack.tp.t_maxseg).min(self.pace_min_segs.max(1));
        let max_val = if self.pace_max_segs != 0 {
            self.pace_max_segs
        } else if self.user_set_max_segs != 0 {
            self.user_set_max_segs.saturating_mul(segsiz)
        } else {
            len
        };
        let mut len = len;
        let mut tso = len > segsiz;
        if len > max_val {
            len = max_val;
        }
        if tso && len <= segsiz {
            tso = false;
        }
        if !tso && len > segsiz {
            len = segsiz;
        }
        if len == 0 || len <= MHLEN.saturating_sub(hdrlen).saturating_sub(MAX_LINKHDR) {
            return Err(RetransmitError::TooShort);
        }
        if entry.flags & RACK_HAD_PUSH != 0 && len == entry.len() {
            flags |= TH_PUSH;
        }
        let seq = entry.start;
        if entry.flags & RACK_TLP != 0 {
            self.tlp_retran = self.tlp_retran.saturating_add(1);
            self.tlp_retran_bytes = self.tlp_retran_bytes.saturating_add(u64::from(len));
        } else {
            self.sndrexmitpack = self.sndrexmitpack.saturating_add(1);
            self.sndrexmitbyte = self.sndrexmitbyte.saturating_add(u64::from(len));
        }
        self.prr_sndcnt = self.prr_sndcnt.saturating_sub(len);
        if self.in_fast_recovery {
            self.retran_during_recovery = self.retran_during_recovery.saturating_add(len);
        }
        let updates = self.update_logged_output(seq, len, now_us, segsiz);
        self.stack.tp.snd_nxt = self.stack.tp.snd_max;
        if self.t_rtttime == 0 {
            self.t_rtttime = u32::try_from(now_us).unwrap_or(u32::MAX).max(1);
            self.t_rtseq = seq;
        }
        self.fto_rsm_send = self.fto_rsm_send.saturating_add(1);
        let _ = self
            .hpts
            .rack_timer_start(self.id, PACE_TMR_RXT, u64::from(self.t_rxtcur.max(1)));
        if doing_tlp {
            self.tlp_in_progress = true;
            self.tlp_cnt_out = self.tlp_cnt_out.saturating_add(1);
        }
        Ok(RackTransmission {
            seq,
            len,
            flags,
            tso,
            segsiz,
            tlp: doing_tlp,
            map_updates: updates,
        })
    }

    fn update_logged_output(&mut self, seq: u32, len: u32, now_us: u64, segsiz: u32) -> u32 {
        let mut cursor = seq;
        let mut remaining = len;
        let mut updates: u32 = 0;
        while remaining > 0 {
            let Some((id, start, end)) = self
                .map
                .covering(cursor)
                .map(|entry| (entry.id, entry.start, entry.end))
            else {
                break;
            };
            if start == cursor {
                let result = self.rack_update_entry(id, now_us, remaining, RACK_SENT_FP, segsiz);
                updates = updates.saturating_add(1);
                if result.remaining == 0 {
                    break;
                }
                remaining = result.remaining;
                cursor = result.seq_out;
            } else {
                let tail = self.map.insert(cursor, end, now_us);
                self.map.clone_tail(tail, id);
                if let Some(entry) = self.map.find_id_mut(id) {
                    entry.end = cursor;
                }
                self.map.clear_edge_flags(id);
                let result = self.rack_update_entry(tail, now_us, remaining, RACK_SENT_FP, segsiz);
                updates = updates.saturating_add(1);
                if result.remaining == 0 {
                    break;
                }
                remaining = result.remaining;
                cursor = result.seq_out;
            }
        }
        updates
    }

    fn rack_rexmtval(&self) -> u32 {
        ((self.srtt >> 3).saturating_add(self.rttvar << 2))
            .max(self.rto_min)
            .min(self.rto_max)
    }

    fn current_thresh(&mut self, now_us: u64) -> u32 {
        let srtt = if self.rack_min_rtt != 0 {
            self.rack_min_rtt
        } else {
            (self.srtt >> 3).max(1)
        };
        let mut thresh = if self.std_based {
            srtt.saturating_add(srtt >> 2)
        } else {
            srtt.saturating_add(self.pkt_delay)
        };
        if self.reorder_ts != 0 {
            let faded = self.reorder_fade != 0
                && u32::try_from(now_us)
                    .unwrap_or(u32::MAX)
                    .saturating_sub(self.reorder_ts)
                    > self.reorder_fade;
            if faded {
                self.reorder_ts = 0;
            } else if !self.std_based {
                thresh = thresh.saturating_add(if self.reorder_shift != 0 {
                    srtt >> self.reorder_shift
                } else {
                    srtt >> 2
                });
            }
            if self.num_dsack > 0 {
                thresh = thresh.saturating_add(self.num_dsack.saturating_mul(srtt >> 2));
            }
        }
        thresh.min(srtt.saturating_mul(2)).min(self.rto_max)
    }

    fn enter_fast_recovery(&mut self, _th_ack: u32) {
        self.in_fast_recovery = true;
        self.snd_recover = self.stack.tp.snd_max;
        self.prr_delivered = 0;
        self.prr_out = 0;
        self.fast_output = false;
        self.recovery_rxt_cnt = 0;
        if !self.no_prr {
            self.prr_sndcnt = u32::from(self.stack.tp.t_maxseg);
        }
        self.prr_recovery_fs = self.outstanding();
    }

    fn finish_recovery(&mut self) {
        if self.stack.tp.snd_cwnd < self.stack.tp.snd_ssthresh {
            self.stack.tp.snd_cwnd = self.stack.tp.snd_ssthresh;
        }
        if !self.no_prr && self.prr_sndcnt > 0 {
            let addback = u32::from(self.stack.tp.t_maxseg)
                .saturating_mul(2_u32)
                .min(self.prr_sndcnt);
            self.stack.tp.snd_cwnd = self.stack.tp.snd_cwnd.saturating_add(addback);
            self.prr_sndcnt = 0;
        }
        self.snd_recover = self.stack.tp.snd_una;
        self.in_fast_recovery = false;
    }
}

/// `tcp_outflags` for the states this group can reach.
fn tcp_outflags(state: u8) -> u16 {
    match state {
        TCPS_SYN_SENT => TH_SYN,
        TCPS_SYN_RECEIVED => TH_SYN | TH_ACK,
        TCPS_ESTABLISHED | TCPS_CLOSE_WAIT => TH_ACK,
        TCPS_FIN_WAIT_1 | TCPS_FIN_WAIT_2 | TCPS_CLOSING | TCPS_LAST_ACK => TH_FIN | TH_ACK,
        _ => TH_ACK,
    }
}

/// `tcp_compute_initwnd` with the FreeBSD default `V_tcp_initcwnd_segments = 10`.
fn rc_init_window(maxseg: u32) -> u32 {
    let maxseg = maxseg.max(1);
    let segments = 10u32;
    segments.saturating_mul(maxseg).min(
        2u32.saturating_mul(maxseg)
            .max(segments.saturating_mul(1460)),
    )
}

fn seq_gt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

fn seq_ge(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) >= 0
}

fn seq_lt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}

fn seq_le(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) <= 0
}

#[cfg(test)]
#[path = "rack_retransmit_ut.rs"]
mod tests;
