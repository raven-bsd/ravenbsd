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

/// `RACK_NUM_OF_RETRANS`, the transmitted-time slots kept per send-map entry.
pub const RACK_NUM_OF_RETRANS: usize = 3;
/// `DUP_ACK_THRESHOLD`, the duplicate-ACK threshold.
pub const DUP_ACK_THRESHOLD: u8 = 3;
/// `MHLEN`, the mbuf header length used by the fast-output size guard.
pub const MHLEN: u32 = 224;
/// `max_linkhdr`, the link-layer header space used by the fast-output size guard.
pub const MAX_LINKHDR: u32 = 14;
/// `PACE_MAX_IP_BYTES`, the pacing-size ceiling.
pub const PACE_MAX_IP_BYTES: u32 = 65_535;
/// `rack_rto_min`, 30 ms in microseconds.
pub const RACK_RTO_MIN: u32 = 30_000;
/// `rack_rto_max`, 4 s in microseconds.
pub const RACK_RTO_MAX: u32 = 4_000_000;
/// `CUM_ACKED`, a cumulative-ACK RTT sample.
pub const CUM_ACKED: u8 = 1;
/// `SACKED`, a SACK RTT sample.
pub const SACKED: u8 = 2;

/// The remote endpoint ACKed this send-map entry.
pub const RACK_ACKED: u32 = 0x0000_0001;
/// A timeout occurred on this send-map entry (`RACK_TO_REXT`).
pub const RACK_TO_REXT: u32 = 0x0000_0002;
/// More retransmissions than the transmit-time array can hold.
pub const RACK_OVERMAX: u32 = 0x0000_0008;
/// A SACK was done above this block.
pub const RACK_SACK_PASSED: u32 = 0x0000_0010;
/// This entry was retransmitted because of a SACK pass.
pub const RACK_WAS_SACKPASS: u32 = 0x0000_0020;
/// The segment carries a FIN.
pub const RACK_HAS_FIN: u32 = 0x0000_0040;
/// The segment was sent as a tail-loss probe.
pub const RACK_TLP: u32 = 0x0000_0080;
/// The peer collapsed the receive window on this segment.
pub const RACK_RWND_COLLAPSED: u32 = 0x0000_0100;
/// The sender became application limited after this segment.
pub const RACK_APP_LIMITED: u32 = 0x0000_0200;
/// A retransmission timeout undid the ACK but the RTT was already sampled.
pub const RACK_WAS_ACKED: u32 = 0x0000_0400;
/// The original transmission carried PUSH.
pub const RACK_HAD_PUSH: u32 = 0x0000_8000;
/// The retransmit timer requires this entry to be sent.
pub const RACK_MUST_RXT: u32 = 0x0001_0000;
/// The path MTU changed on this entry.
pub const RACK_PMTU_CHG: u32 = 0x0010_0000;
/// RACK considers this entry lost.
pub const RACK_WAS_LOST: u32 = 0x0040_0000;
/// A fast-path transmission (`RACK_SENT_FP`).
pub const RACK_SENT_FP: u32 = 0x0000_4000;
/// The TCP PUSH flag.
pub const TH_PUSH: u16 = 0x08;

/// One `struct rack_sendmap` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rsm {
    /// Stable entry identifier (the donor pointer identity).
    pub id: u32,
    /// First sequence number.
    pub r_start: u32,
    /// One past the last sequence number.
    pub r_end: u32,
    /// Retransmitted bytes.
    pub r_rtr_bytes: u32,
    /// `RACK_*` flags.
    pub r_flags: u32,
    /// Number of transmit-time slots used.
    pub r_rtr_cnt: u8,
    /// Total number of transmissions.
    pub r_act_rxt_cnt: u32,
    /// Duplicate-ACK count.
    pub r_dupack: u8,
    /// Whether the entry is linked in the time-order list.
    pub r_in_tmap: bool,
    /// RTT sampling is disabled for this entry.
    pub r_no_rtt_allowed: bool,
    /// KTLS transmit state.
    pub r_hw_tls: bool,
    /// Transmit timestamps, µs.
    pub r_tim_lastsent: [u64; RACK_NUM_OF_RETRANS],
    /// ACK arrival timestamp, µs.
    pub r_ack_arrival: u64,
    /// Flight size at send.
    pub r_fas: u32,
    /// Burst size at send.
    pub r_bas: u8,
}

impl Rsm {
    /// Create an entry with one transmit timestamp.
    #[must_use]
    pub const fn new(id: u32, start: u32, end: u32, ts: u64) -> Self {
        let mut r_tim_lastsent = [0; RACK_NUM_OF_RETRANS];
        r_tim_lastsent[0] = ts;
        Self {
            id,
            r_start: start,
            r_end: end,
            r_rtr_bytes: 0,
            r_flags: 0,
            r_rtr_cnt: 1,
            r_act_rxt_cnt: 0,
            r_dupack: 0,
            r_in_tmap: true,
            r_no_rtt_allowed: false,
            r_hw_tls: false,
            r_tim_lastsent,
            r_ack_arrival: 0,
            r_fas: 0,
            r_bas: 0,
        }
    }

    /// Segment length in bytes.
    #[must_use]
    pub const fn len(&self) -> u32 {
        self.r_end.saturating_sub(self.r_start)
    }

    /// Whether the entry covers no sequence space.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.r_end <= self.r_start
    }

    /// The most recent transmit timestamp.
    #[must_use]
    pub const fn last_sent(&self) -> u64 {
        let index = if self.r_rtr_cnt == 0 {
            0
        } else {
            self.r_rtr_cnt as usize - 1
        };
        self.r_tim_lastsent[index]
    }
}

/// The sequence-ordered send map with the donor time-order list.
#[derive(Clone, Debug, Default)]
pub struct RackSendMap {
    /// Sequence-ordered entries.
    pub segments: Vec<Rsm>,
    /// Time-order entry identifiers (`rc_tmap`).
    pub tmap: Vec<u32>,
    next_id: u32,
}

impl RackSendMap {
    /// Insert a new transmitted segment and append it to the time-order list.
    pub fn insert(&mut self, start: u32, end: u32, ts: u64) -> u32 {
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        let index = self.segments.partition_point(|entry| entry.r_start < start);
        self.segments.insert(index, Rsm::new(id, start, end, ts));
        self.tmap.push(id);
        id
    }

    /// Find an entry by sequence start.
    #[must_use]
    pub fn find(&self, start: u32) -> Option<&Rsm> {
        self.segments.iter().find(|entry| entry.r_start == start)
    }

    /// Find an entry by identifier.
    #[must_use]
    pub fn find_id(&self, id: u32) -> Option<&Rsm> {
        self.segments.iter().find(|entry| entry.id == id)
    }

    /// Mutably find an entry by identifier.
    pub fn find_id_mut(&mut self, id: u32) -> Option<&mut Rsm> {
        self.segments.iter_mut().find(|entry| entry.id == id)
    }

    /// Find the entry covering `seq`.
    #[must_use]
    pub fn covering(&self, seq: u32) -> Option<&Rsm> {
        self.segments
            .iter()
            .find(|entry| seq_ge(seq, entry.r_start) && seq_lt(seq, entry.r_end))
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
        if entry.r_flags & RACK_ACKED != 0 {
            return false;
        }
        entry.r_flags |= RACK_ACKED;
        *sacked = sacked.saturating_add(entry.len());
        true
    }

    /// Remove entries covered by a cumulative ACK and trim a partial head.
    pub fn ack_through(&mut self, ack: u32, sacked: &mut u32) {
        let mut removed = Vec::new();
        for entry in &mut self.segments {
            if seq_le(entry.r_end, ack) {
                if entry.r_flags & RACK_ACKED != 0 {
                    *sacked = sacked.saturating_sub(entry.len());
                }
                removed.push(entry.id);
            } else if seq_lt(entry.r_start, ack) {
                if entry.r_flags & RACK_ACKED != 0 {
                    *sacked = sacked.saturating_sub(ack.wrapping_sub(entry.r_start));
                }
                entry.r_start = ack;
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
        tail.r_rtr_cnt = source.r_rtr_cnt;
        tail.r_act_rxt_cnt = source.r_act_rxt_cnt;
        tail.r_flags = source.r_flags;
        tail.r_dupack = source.r_dupack;
        tail.r_no_rtt_allowed = source.r_no_rtt_allowed;
        tail.r_hw_tls = source.r_hw_tls;
        tail.r_rtr_bytes = 0;
        tail.r_fas = source.r_fas;
        tail.r_bas = source.r_bas;
        tail.r_tim_lastsent = source.r_tim_lastsent;
        tail.r_flags &= !0x0000_0800; // RACK_HAS_SYN stays on the left edge.
    }

    /// Clear the right-edge flags after a split.
    fn clear_edge_flags(&mut self, id: u32) {
        if let Some(entry) = self.find_id_mut(id) {
            entry.r_flags &= !(RACK_HAS_FIN | RACK_HAD_PUSH);
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
        }
    }

    /// Record a new transmission.
    pub fn send(&mut self, start: u32, end: u32, now_us: u64) -> u32 {
        let id = self.map.insert(start, end, now_us);
        let flight = self.flight_size();
        if let Some(entry) = self.map.find_id_mut(id) {
            entry.r_fas = flight;
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
                .is_some_and(|entry| entry.r_flags & RACK_ACKED == 0)
        })
    }

    /// `rack_find_high_nonack`: the highest sequence not ACKed or FIN, at or below `id`.
    #[must_use]
    pub fn rack_find_high_nonack(&self, id: u32) -> Option<u32> {
        let start = self.map.segments.iter().position(|entry| entry.id == id)?;
        self.map.segments[..=start]
            .iter()
            .rev()
            .find(|entry| entry.r_flags & (RACK_ACKED | RACK_HAS_FIN) == 0)
            .map(|entry| entry.id)
    }

    /// `rack_update_rsm`: account one retransmission of a send-map entry.
    pub fn rack_update_rsm(&mut self, id: u32, ts: u64, _add_flag: u32, segsiz: u32) {
        let flight = self.flight_size();
        let Some(entry) = self.map.find_id_mut(id) else {
            return;
        };
        entry.r_rtr_cnt = entry.r_rtr_cnt.saturating_add(1);
        if entry.r_rtr_cnt as usize > RACK_NUM_OF_RETRANS {
            entry.r_rtr_cnt = RACK_NUM_OF_RETRANS as u8;
            entry.r_flags |= RACK_OVERMAX;
        }
        entry.r_act_rxt_cnt = entry.r_act_rxt_cnt.saturating_add(1);
        entry.r_dupack = 0;
        let len = entry.len();
        let mut holes = 0;
        if entry.r_rtr_cnt > 1 && entry.r_flags & RACK_TLP == 0 {
            holes = len;
            entry.r_rtr_bytes = entry.r_rtr_bytes.saturating_add(len);
        }
        let was_lost = entry.r_flags & RACK_WAS_LOST != 0;
        if was_lost {
            entry.r_flags &= !RACK_WAS_LOST;
        }
        let index = entry.r_rtr_cnt as usize - 1;
        entry.r_tim_lastsent[index] = ts;
        entry.r_fas = flight;
        let was_acked = entry.r_flags & RACK_ACKED != 0;
        if was_acked {
            entry.r_flags &= !RACK_ACKED;
        }
        entry.r_in_tmap = true;
        let segsiz = segsiz.max(1);
        entry.r_bas = u8::try_from(len.div_ceil(segsiz)).unwrap_or(u8::MAX);
        let must_rxt = entry.r_flags & RACK_MUST_RXT != 0;
        let reaches_rto_max = seq_ge(entry.r_end, self.snd_max_at_rto);
        if must_rxt {
            entry.r_flags &= !RACK_MUST_RXT;
        }
        entry.r_flags &= !RACK_RWND_COLLAPSED;
        let sack_passed = entry.r_flags & RACK_SACK_PASSED != 0;
        if sack_passed {
            entry.r_flags &= !RACK_SACK_PASSED;
            entry.r_flags |= RACK_WAS_SACKPASS;
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
        let start = entry.r_start;
        let end = entry.r_end;
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
            entry.r_end = c_end;
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
        if entry.r_flags & (RACK_ACKED | RACK_WAS_ACKED) != 0 || entry.r_no_rtt_allowed {
            return None;
        }
        let last = entry.last_sent();
        let count = entry.r_rtr_cnt;
        let _len_acked = if ack_type == CUM_ACKED {
            if seq_gt(th_ack, entry.r_end) {
                entry.len()
            } else {
                th_ack.wrapping_sub(entry.r_start)
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
                entry.r_dupack < DUP_ACK_THRESHOLD && entry.r_flags & RACK_MUST_RXT == 0
            })
        })?;
        if let Some(entry) = self.map.find_id_mut(id)
            && entry.r_dupack < u8::MAX
        {
            entry.r_dupack = entry.r_dupack.saturating_add(1);
        }
        let ready = self
            .map
            .find_id(id)
            .is_some_and(|entry| entry.r_dupack >= DUP_ACK_THRESHOLD);
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
            .is_some_and(|entry| entry.r_flags & RACK_ACKED != 0)
        {
            id = self.rack_find_lowest_rsm()?;
        }
        let entry = self.map.find_id(id)?;
        if self.must_retran && entry.r_flags & RACK_MUST_RXT != 0 {
            return Some(id);
        }
        if !self.sack_permit && entry.r_dupack >= DUP_ACK_THRESHOLD {
            return Some(id);
        }
        if entry.r_flags & RACK_ACKED != 0 {
            return None;
        }
        if entry.r_flags & RACK_SACK_PASSED == 0 && entry.r_dupack < DUP_ACK_THRESHOLD {
            return None;
        }
        let low = entry.last_sent();
        if now_us <= low || now_us.saturating_sub(low) < u64::from(thresh) {
            return None;
        }
        if entry.r_dupack >= DUP_ACK_THRESHOLD || entry.r_flags & RACK_SACK_PASSED != 0 {
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
                entry.r_flags |= RACK_TLP;
            }
        } else if let Some(entry) = self.map.find_id_mut(id) {
            entry.r_flags &= !RACK_TLP;
        }
        let entry = self.map.find_id(id).ok_or(RetransmitError::NoSegment)?;
        let mut flags = tcp_outflags(self.stack.tp.t_state);
        if flags & (TH_SYN | TH_RST) != 0 || entry.r_flags & RACK_HAS_FIN != 0 {
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
        if entry.r_flags & RACK_HAD_PUSH != 0 && len == entry.len() {
            flags |= TH_PUSH;
        }
        let seq = entry.r_start;
        if entry.r_flags & RACK_TLP != 0 {
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
                .map(|entry| (entry.id, entry.r_start, entry.r_end))
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
                    entry.r_end = cursor;
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
