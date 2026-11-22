// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD `sys/netinet/tcp_stacks/rack.c` at
// 133344405f532bd54ff3629a77d4c4e062f01fdb (BSD-2-Clause): init/fini,
// sendmap allocation, clone/merge, socket options, inheritance, MTU/send-buffer
// adjustment and the query/stack-information surface. The kernel pointers,
// kmem zones, sockopt copyin/copyout and locks are mapped to the K314 `RackOs`
// software context; no C expression is copied.

//! RACK init/alloc/sockopt/misc port.
//!
//! This module owns the non-recovery donor functions listed in the K334 report.
//! It uses the K313 HPTS/timer state, the K314 [`RackOs`]/[`RackTcpcb`] context
//! and the K318 socket-option/query shapes. No stack wiring is added here.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use core::mem::size_of;

use crate::rack_os::{MbufId, RackOs, RackTcpcb};
use crate::rack_types::{
    RACK_APP_LIMITED, RACK_HAD_PUSH, RACK_HAS_FIN, RACK_HAS_SYN, RACK_MUST_RXT,
    RACK_NUM_OF_RETRANS, RACK_PMTU_CHG, RACK_RWND_COLLAPSED, RACK_TLP, RackSendmap,
};
use crate::rack_vtable::{
    IPPROTO_TCP, PRUS_OOB, RackSockopt, SockoptDir, TCP_INFO, TCP_PACING_DND, TCP_QUERY_RACK_TIMES,
    TCP_QUERY_SENDMAP, TCP_QUERY_TIMERS_UP, TCP_RACK_PACE_MAX_SEG, TCP_RACK_PACE_MIN_SEG,
    TCP_RACK_PACING_BETA, TCP_RACK_PACING_DIVISOR, TCPS_CLOSED, TH_ACK, TH_FIN, TH_RST,
};
use crate::{Errno, RackStackInfo};

/// FreeBSD `rack_free_cache` default.
pub const RACK_FREE_CACHE: usize = 4;
/// FreeBSD `RACK_FREE_CNT_MAX`.
pub const RACK_FREE_CNT_MAX: usize = 8;
/// Largest deferred-option list the model accepts.
pub const RACK_DEFERRED_MAX: usize = 16;

/// `TCP_RACK_PROFILE`.
pub const TCP_RACK_PROFILE: i32 = 1129;
/// `TCP_DEFER_OPTIONS`.
pub const TCP_DEFER_OPTIONS: i32 = 1136;
/// `TCP_RACK_DSACK_OPT`.
pub const TCP_RACK_DSACK_OPT: i32 = 1141;
/// `TCP_RACK_SET_RXT_OPTIONS`.
pub const TCP_RACK_SET_RXT_OPTIONS: i32 = 1143;
/// `TCP_RACK_HI_BETA`.
pub const TCP_RACK_HI_BETA: i32 = 1144;
/// `TCP_RACK_TIMER_SLOP`.
pub const TCP_RACK_TIMER_SLOP: i32 = 1140;
/// `TCP_RACK_MEASURE_CNT`.
pub const TCP_RACK_MEASURE_CNT: i32 = 1135;
/// `TCP_NO_PRR`.
pub const TCP_NO_PRR: i32 = 1122;
/// `TCP_USE_CMP_ACKS`.
pub const TCP_USE_CMP_ACKS: i32 = 77;
/// `TCP_RACK_MBUF_QUEUE`.
pub const TCP_RACK_MBUF_QUEUE: i32 = 1050;
/// `TCP_RACK_PACE_TO_FILL`.
pub const TCP_RACK_PACE_TO_FILL: i32 = 1127;
/// `TCP_RACK_FORCE_MSEG`.
pub const TCP_RACK_FORCE_MSEG: i32 = 1117;
/// `TCP_RACK_ABC_VAL`.
pub const TCP_RACK_ABC_VAL: i32 = 1133;

/// The fast-send-block header state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FsbBlock {
    /// `tcp_ip_hdr_len`.
    pub header_len: usize,
    /// `r_is_v6`.
    pub ipv6: bool,
    /// `tp->t_port` is nonzero.
    pub tunnel: bool,
    /// `recwin`.
    pub recwin: u32,
    /// `r_fsb_inited`.
    pub initialized: bool,
    /// `rack->r_ctl.fsb.hw_tls`.
    pub hw_tls: bool,
    /// The header backing store.
    pub mbuf: Option<MbufId>,
}

/// One deferred socket option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeferredOption {
    /// `optname`.
    pub name: i32,
    /// `optval`.
    pub value: u64,
}

/// The donor `tcp_query_resp` surface needed by the misc port.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MiscQuery {
    /// `req`.
    pub req: i32,
    /// `req_param`.
    pub req_param: u32,
    /// `sendmap_start`.
    pub sendmap_start: u32,
    /// `sendmap_end`.
    pub sendmap_end: u32,
    /// `sendmap_send_cnt`.
    pub sendmap_send_cnt: u8,
    /// `sendmap_fas`.
    pub sendmap_fas: u32,
    /// `sendmap_time`.
    pub sendmap_time: [u64; RACK_NUM_OF_RETRANS],
    /// `sendmap_ack_arrival`.
    pub sendmap_ack_arrival: u64,
    /// `sendmap_flags`.
    pub sendmap_flags: u32,
    /// `sendmap_r_rtr_bytes`.
    pub sendmap_r_rtr_bytes: u32,
    /// `sendmap_dupacks`.
    pub sendmap_dupacks: u8,
    /// `timer_hpts_flags`.
    pub timer_hpts_flags: u32,
    /// `timer_pacing_to`.
    pub timer_pacing_to: i64,
    /// `timer_timer_exp`.
    pub timer_timer_exp: i64,
    /// `rack_num_dsacks`.
    pub rack_num_dsacks: u32,
    /// `rack_reorder_ts`.
    pub rack_reorder_ts: u32,
    /// `rack_rxt_last_time`.
    pub rack_rxt_last_time: u32,
    /// `rack_min_rtt`.
    pub rack_min_rtt: u32,
    /// `rack_rtt`.
    pub rack_rtt: u32,
    /// `rack_tmit_time`.
    pub rack_tmit_time: u32,
    /// `rack_srtt_measured`.
    pub rack_srtt_measured: u32,
    /// `rack_sacked`.
    pub rack_sacked: u32,
    /// `rack_holes_rxt`.
    pub rack_holes_rxt: u32,
    /// `rack_prr_delivered`.
    pub rack_prr_delivered: u32,
    /// `rack_prr_recovery_fs`.
    pub rack_prr_recovery_fs: u32,
    /// `rack_prr_sndcnt`.
    pub rack_prr_sndcnt: u32,
    /// `rack_prr_out`.
    pub rack_prr_out: u32,
    /// `rack_tlp_out`.
    pub rack_tlp_out: u8,
    /// `rack_tlp_cnt_out`.
    pub rack_tlp_cnt_out: u32,
    /// `rack_time_went_idle`.
    pub rack_time_went_idle: u32,
    /// `rack_in_persist`.
    pub rack_in_persist: u8,
    /// `rack_wanted_output`.
    pub rack_wanted_output: u8,
}

/// A `tcp_info` snapshot for `rack_fill_info`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RackTcpInfo {
    /// `tcpi_state`.
    pub state: u8,
    /// `tcpi_options`.
    pub options: u32,
    /// `tcpi_rtt`.
    pub rtt: u32,
    /// `tcpi_rttvar`.
    pub rttvar: u32,
    /// `tcpi_rto`.
    pub rto: u32,
    /// `tcpi_snd_ssthresh`.
    pub snd_ssthresh: u32,
    /// `tcpi_snd_cwnd`.
    pub snd_cwnd: u32,
    /// `tcpi_rcv_space`.
    pub rcv_space: u32,
    /// `tcpi_rcv_nxt`.
    pub rcv_nxt: u32,
    /// `tcpi_snd_wnd`.
    pub snd_wnd: u32,
    /// `tcpi_snd_nxt`.
    pub snd_nxt: u32,
    /// `tcpi_snd_mss`.
    pub snd_mss: u16,
    /// `tcpi_rcv_mss`.
    pub rcv_mss: u16,
    /// `tcpi_snd_rexmitpack`.
    pub snd_rexmitpack: u32,
    /// `tcpi_rcv_ooopack`.
    pub rcv_ooopack: u32,
    /// `tcpi_snd_zerowin`.
    pub snd_zerowin: u32,
    /// `tcpi_total_tlp`.
    pub total_tlp: u32,
    /// `tcpi_total_tlp_bytes`.
    pub total_tlp_bytes: u32,
    /// `tcpi_rttmin`.
    pub rttmin: u32,
}

/// The RACK misc state over the K314 context.
pub struct RackMisc {
    /// K314 buffers/zones/HPTS/pacing.
    pub os: RackOs,
    /// `tcpcb` mirror.
    pub tp: RackTcpcb,
    /// Sendmap zone.
    pub sendmap_zone: u32,
    /// `tcp_rack` zone.
    pub pcb_zone: u32,
    /// Active sendmaps.
    pub sendmaps: BTreeMap<u64, RackSendmap>,
    /// Free list.
    pub free: Vec<u64>,
    /// `rc_free_cnt`.
    pub free_count: usize,
    /// `rc_num_maps_alloced`.
    pub maps_alloced: u32,
    /// `rc_num_split_allocs`.
    pub split_allocs: u32,
    /// `rc_split_limit`.
    pub split_limit: u32,
    /// `V_tcp_map_entries_limit`.
    pub map_entries_limit: u32,
    /// `alloc_limit_reported`.
    pub alloc_limit_reported: bool,
    /// `r_is_v6`.
    pub ipv6: bool,
    /// `tp->t_port != 0`.
    pub tunnel: bool,
    /// `iss`.
    pub iss: u32,
    /// `snd_wnd`.
    pub snd_wnd: u32,
    /// `snd_cwnd` copy used by autoscaling.
    pub snd_cwnd: u32,
    /// `rc_fsb`.
    pub fsb: FsbBlock,
    /// `r_fsb_inited`.
    pub fsb_inited: bool,
    /// `rack_deferred_inited`.
    pub deferred_inited: bool,
    /// `r_ctl.roundends`.
    pub roundends: u32,
    /// `r_ctl.rc_high_rwnd`.
    pub high_rwnd: u32,
    /// `r_ctl.cwnd_to_use`.
    pub cwnd_to_use: u32,
    /// `full_size_rxt`.
    pub full_size_rxt: bool,
    /// `shape_rxt_to_pacing_min`.
    pub shape_rxt_to_pacing_min: bool,
    /// `defer_options`.
    pub defer_options: bool,
    /// Deferred options.
    pub deferred: VecDeque<DeferredOption>,
    /// `r_ctl.pace_len_divisor`.
    pub pacing_divisor: u32,
    /// `r_ctl.rc_user_set_min_segs`.
    pub min_segs: u16,
    /// `rc_user_set_max_segs`.
    pub max_segs: u16,
    /// `rc_force_max_seg`.
    pub force_max_seg: bool,
    /// `rc_pace_to_cwnd`.
    pub pace_to_cwnd: bool,
    /// `dgp_on`.
    pub dgp_on: bool,
    /// `rc_always_pace`.
    pub always_pace: bool,
    /// `r_use_cmp_ack`.
    pub use_cmp_ack: bool,
    /// `r_mbuf_queue`.
    pub mbuf_queue: bool,
    /// `rc_rack_tmr_std_based`.
    pub dsack_std_based: bool,
    /// `rc_rack_use_dsack`.
    pub dsack_use: bool,
    /// `r_ctl.req_measurements`.
    pub req_measurements: u32,
    /// `rack_hibeta`.
    pub hi_beta: bool,
    /// `r_ctl.timer_slop`.
    pub timer_slop: u32,
    /// `rack_no_prr`.
    pub no_prr: bool,
    /// `rc_labc`.
    pub abc_val: u8,
    /// `rc_pace_dnd`.
    pub pacing_dnd: bool,
    /// `r_profile`.
    pub profile: i32,
    /// `t_sndbytes`.
    pub bytes_transmitted: u64,
    /// `t_snd_rxt_bytes`.
    pub bytes_retransmitted: u64,
    /// `rc_app_limited_cnt`.
    pub app_limited_cnt: u32,
    /// `SB_AUTOSIZE`.
    pub sndbuf_autosize: bool,
    /// `tcp_autosndbuf_inc`.
    pub autosndbuf_inc: u32,
    /// `tcp_autosndbuf_max`.
    pub autosndbuf_max: u32,
    /// `initialized`.
    pub initialized: bool,
    /// Saved NewReno beta.
    pub saved_beta: u32,
    /// Saved NewReno ECN beta.
    pub saved_beta_ecn: u32,
    /// `rc_pacing_cc_set`.
    pub pacing_cc_set: bool,
    /// The CC is NewReno.
    pub cc_is_newreno: bool,
    /// Recovery multiplier (`rack_per_of_gp_rec`).
    pub per_gp_rec: u32,
    /// Congestion-avoidance multiplier.
    pub per_gp_ca: u32,
    /// Slow-start multiplier.
    pub per_gp_ss: u32,
    /// Dynamic multiplier enabled.
    pub gp_dyn_mul: bool,
    /// A fixed rate is configured.
    pub use_fixed_rate: bool,
    /// Probe-RTT is active.
    pub in_probe_rtt: bool,
    /// Last computed goodput bandwidth.
    pub last_gp_comp_bw: u64,
    /// Loss counter.
    pub loss_count: u32,
    /// Loss counter at the start of the measurement.
    pub loss_at_start: u32,
    /// Bandwidth reduction is active.
    pub gp_bwred: bool,
    /// Timely decrease count.
    pub gp_timely_dec_cnt: u32,
    /// No-push-at-min-RTT mode.
    pub no_push_at_mrtt: u32,
    /// PRR send count.
    pub prr_sndcnt: u32,
    /// Output is wanted.
    pub wanted_output: bool,
    /// Lowest RTT in the ACK.
    pub rtt_lowest: u32,
    /// Highest RTT in the ACK.
    pub rtt_highest: u32,
    /// RTT total.
    pub rtt_tot: u64,
    /// RTT sample count.
    pub rtt_cnt: u32,
    /// A valid RTT sample exists.
    pub rtt_valid: bool,
    /// Best microsecond RTT.
    pub best_us_rtt: u32,
    /// Confidence of `best_us_rtt`.
    pub rtt_confidence: u8,
    /// Retransmit count of `best_us_rtt`.
    pub best_rtr_cnt: u16,
    /// Rate sample method (0 lowest, 1 highest, 2 average).
    pub rate_sample_method: u8,
    /// Minimum RTT filter.
    pub min_rtt_us: u32,
    /// Time of the last lower RTT.
    pub lower_rtt_us_cts: u32,
    /// Last TLP ACKed start.
    pub last_tlp_acked_start: u32,
    /// Last TLP ACKed end.
    pub last_tlp_acked_end: u32,
    /// Goodput measurement is in progress.
    pub gput_inprog: bool,
    /// Goodput ACK.
    pub gput_ack: u32,
    /// Goodput cumulative ACK timestamp.
    pub gp_cumack_ts: u64,
    /// App-limited needs set.
    pub app_limited_needs_set: bool,
    /// Dragged bottom flag.
    pub dragged_bottom: bool,
    /// Current round.
    pub current_round: u32,
    /// Last round ACK.
    pub last_round_ack: u32,
    /// Fast output is in progress.
    pub fast_output: bool,
    /// Bytes left to send in fast output.
    pub fsb_left_to_send: u32,
    /// Fast output has started.
    pub fast_output_started: bool,
    /// Data after close is allowed.
    pub allow_data_after_close: bool,
    /// Pending request completions.
    pub reqs: Vec<(u32, u32)>,
    /// Forced ACK is active.
    pub forced_ack: bool,
    /// Forced ACK timestamp.
    pub forced_ack_ts: u64,
    /// Probe was not answered.
    pub probe_not_answered: bool,
    /// A window collapse is active.
    pub has_collapsed: bool,
    /// Last collapse point.
    pub last_collapse_point: u32,
    /// High collapse point.
    pub high_collapse_point: u32,
    /// Collapse point is valid.
    pub collapse_point_valid: bool,
}

impl Default for RackMisc {
    fn default() -> Self {
        Self::new()
    }
}

impl RackMisc {
    /// Create the model with the two UMA zones and the FreeBSD defaults.
    #[must_use]
    pub fn new() -> Self {
        let mut os = RackOs::new();
        let sendmap_zone = os.uma_zcreate("rack_map", size_of::<RackSendmap>());
        let pcb_zone = os.uma_zcreate("rack_pcb", size_of::<RackMisc>());
        Self {
            os,
            tp: RackTcpcb::default(),
            sendmap_zone,
            pcb_zone,
            sendmaps: BTreeMap::new(),
            free: Vec::new(),
            free_count: 0,
            maps_alloced: 0,
            split_allocs: 0,
            split_limit: 0,
            map_entries_limit: 0,
            alloc_limit_reported: false,
            ipv6: false,
            tunnel: false,
            iss: 0,
            snd_wnd: 65_535,
            snd_cwnd: 10,
            fsb: FsbBlock::default(),
            fsb_inited: false,
            deferred_inited: false,
            roundends: 0,
            high_rwnd: 0,
            cwnd_to_use: 0,
            full_size_rxt: false,
            shape_rxt_to_pacing_min: false,
            defer_options: false,
            deferred: VecDeque::new(),
            pacing_divisor: crate::RACK_DEFAULT_PACING_DIVISOR,
            min_segs: 0,
            max_segs: 0,
            force_max_seg: false,
            pace_to_cwnd: false,
            dgp_on: false,
            always_pace: false,
            use_cmp_ack: false,
            mbuf_queue: false,
            dsack_std_based: false,
            dsack_use: false,
            req_measurements: 1,
            hi_beta: false,
            timer_slop: 0,
            no_prr: false,
            abc_val: 0,
            pacing_dnd: false,
            profile: 0,
            bytes_transmitted: 0,
            bytes_retransmitted: 0,
            app_limited_cnt: 0,
            sndbuf_autosize: true,
            autosndbuf_inc: 1024,
            autosndbuf_max: 2 * 1024 * 1024,
            initialized: false,
            saved_beta: 0,
            saved_beta_ecn: 0,
            pacing_cc_set: false,
            cc_is_newreno: true,
            per_gp_rec: 100,
            per_gp_ca: 100,
            per_gp_ss: 100,
            gp_dyn_mul: false,
            use_fixed_rate: false,
            in_probe_rtt: false,
            last_gp_comp_bw: 0,
            loss_count: 0,
            loss_at_start: 0,
            gp_bwred: false,
            gp_timely_dec_cnt: 0,
            no_push_at_mrtt: 0,
            prr_sndcnt: 0,
            wanted_output: false,
            rtt_lowest: u32::MAX,
            rtt_highest: 0,
            rtt_tot: 0,
            rtt_cnt: 0,
            rtt_valid: false,
            best_us_rtt: u32::MAX,
            rtt_confidence: 0,
            best_rtr_cnt: 0,
            rate_sample_method: 2,
            min_rtt_us: u32::MAX,
            lower_rtt_us_cts: 0,
            last_tlp_acked_start: 0,
            last_tlp_acked_end: 0,
            gput_inprog: false,
            gput_ack: 0,
            gp_cumack_ts: 0,
            app_limited_needs_set: false,
            dragged_bottom: false,
            current_round: 0,
            last_round_ack: 0,
            fast_output: false,
            fsb_left_to_send: 0,
            fast_output_started: false,
            allow_data_after_close: false,
            reqs: Vec::new(),
            forced_ack: false,
            forced_ack_ts: 0,
            probe_not_answered: false,
            has_collapsed: false,
            last_collapse_point: 0,
            high_collapse_point: 0,
            collapse_point_valid: false,
        }
    }

    /// FreeBSD `rack_alloc`.
    pub fn rack_alloc(&mut self) -> Option<u64> {
        if self.free_count > RACK_FREE_CACHE {
            let id = self.free.pop()?;
            self.free_count = self.free_count.saturating_sub(1);
            return Some(id);
        }
        if let Some(item) = self.os.uma_zalloc(self.sendmap_zone) {
            self.maps_alloced = self.maps_alloced.saturating_add(1);
            self.sendmaps.insert(item, RackSendmap::default());
            return Some(item);
        }
        if let Some(id) = self.free.pop() {
            self.free_count = self.free_count.saturating_sub(1);
            return Some(id);
        }
        None
    }

    /// FreeBSD `rack_alloc_full_limit`.
    pub fn rack_alloc_full_limit(&mut self) -> Option<u64> {
        if self.map_entries_limit > 0 && self.maps_alloced >= self.map_entries_limit {
            if !self.alloc_limit_reported {
                self.alloc_limit_reported = true;
            }
            return None;
        }
        self.rack_alloc()
    }

    /// FreeBSD `rack_alloc_limit`.
    pub fn rack_alloc_limit(&mut self, limit_type: u8) -> Option<u64> {
        if limit_type != 0 && self.split_limit > 0 && self.split_allocs >= self.split_limit {
            if !self.alloc_limit_reported {
                self.alloc_limit_reported = true;
            }
            return None;
        }
        let id = self.rack_alloc()?;
        if limit_type != 0 {
            if let Some(map) = self.sendmaps.get_mut(&id) {
                map.limit_type = limit_type;
            }
            self.split_allocs = self.split_allocs.saturating_add(1);
        }
        Some(id)
    }

    /// FreeBSD `rack_free_trim`.
    pub fn rack_free_trim(&mut self) {
        while self.free_count > RACK_FREE_CACHE {
            let Some(id) = self.free.pop() else { break };
            self.free_count = self.free_count.saturating_sub(1);
            self.sendmaps.remove(&id);
            self.maps_alloced = self.maps_alloced.saturating_sub(1);
            let _ = self.os.uma_zfree(self.sendmap_zone, id);
        }
    }

    /// FreeBSD `rack_free`.
    pub fn rack_free(&mut self, id: u64) -> bool {
        let Some(mut map) = self.sendmaps.remove(&id) else {
            return false;
        };
        if map.flags & RACK_APP_LIMITED != 0 {
            self.app_limited_cnt = self.app_limited_cnt.saturating_sub(1);
        }
        if map.limit_type != 0 {
            self.split_allocs = self.split_allocs.saturating_sub(1);
        }
        map = RackSendmap::default();
        self.sendmaps.insert(id, map);
        self.free.push(id);
        self.free_count = self.free_count.saturating_add(1);
        if self.free_count + 1 > RACK_FREE_CNT_MAX {
            self.rack_free_trim();
        }
        true
    }

    /// FreeBSD `rack_clone_rsm`.
    pub fn rack_clone_rsm(&mut self, source: u64, start: u32) -> Option<u64> {
        let original = *self.sendmaps.get(&source)?;
        let id = self.rack_alloc_limit(0)?;
        let mut cloned = original;
        cloned.start = start;
        cloned.rtr_bytes = 0;
        cloned.flags &= !(RACK_HAS_SYN | RACK_HAS_FIN | RACK_HAD_PUSH);
        if original.flags & RACK_HAS_FIN != 0
            && let Some(map) = self.sendmaps.get_mut(&source)
        {
            map.flags &= !RACK_HAS_FIN;
        }
        if original.flags & RACK_APP_LIMITED != 0 {
            self.app_limited_cnt = self.app_limited_cnt.saturating_add(1);
        }
        self.sendmaps.insert(id, cloned);
        Some(id)
    }

    /// FreeBSD `rack_merge_rsm`.
    pub fn rack_merge_rsm(&mut self, left: u64, right: u64) -> bool {
        let Some(right_map) = self.sendmaps.get(&right).cloned() else {
            return false;
        };
        let Some(left_map) = self.sendmaps.get_mut(&left) else {
            return false;
        };
        left_map.end = right_map.end;
        left_map.dupack = left_map.dupack.max(right_map.dupack);
        left_map.rtr_bytes = left_map.rtr_bytes.saturating_add(right_map.rtr_bytes);
        for flag in [RACK_HAS_FIN, RACK_TLP, RACK_RWND_COLLAPSED] {
            if right_map.flags & flag != 0 {
                left_map.flags |= flag;
            }
        }
        if right_map.flags & RACK_APP_LIMITED != 0 {
            left_map.flags |= RACK_APP_LIMITED;
            if let Some(right_map) = self.sendmaps.get_mut(&right) {
                right_map.flags &= !RACK_APP_LIMITED;
            }
        }
        self.sendmaps.remove(&right);
        self.free.push(right);
        self.free_count = self.free_count.saturating_add(1);
        true
    }

    /// FreeBSD `rack_init_fsb_block`.
    pub fn rack_init_fsb_block(&mut self) {
        let tcp_header = 20usize;
        let ip_header = if self.ipv6 { 40 } else { 20 };
        let udp_header = if self.tunnel { 8 } else { 0 };
        self.fsb.header_len = ip_header + tcp_header + udp_header;
        self.fsb.ipv6 = self.ipv6;
        self.fsb.tunnel = self.tunnel;
        self.fsb.recwin = self.os.sbspace().clamp(0, i64::from(u16::MAX) << 14) as u32;
        self.fsb_inited = true;
    }

    /// FreeBSD `rack_init_fsb`.
    pub fn rack_init_fsb(&mut self) -> Result<(), Errno> {
        let header_len = if self.ipv6 { 40 + 20 + 8 } else { 20 + 20 + 8 };
        self.fsb.header_len = header_len;
        self.fsb.mbuf = Some(self.os.m_gethdr().ok_or(Errno::ENOMEM)?);
        self.fsb_inited = false;
        Ok(())
    }

    /// FreeBSD `rack_deferred_init`.
    pub fn rack_deferred_init(&mut self) {
        self.deferred_inited = true;
        self.roundends = self.tp.snd_max;
        self.high_rwnd = self.snd_wnd;
        self.cwnd_to_use = self.tp.snd_cwnd;
    }

    /// FreeBSD `rack_init_retransmit_value`.
    pub fn rack_init_retransmit_value(&mut self, ctl: i32) {
        self.full_size_rxt = ctl == 1;
        self.shape_rxt_to_pacing_min = ctl == 2;
    }

    /// FreeBSD `rack_init_outstanding`.
    pub fn rack_init_outstanding(&mut self) -> Result<u64, Errno> {
        let id = self.rack_alloc().ok_or(Errno::ENOMEM)?;
        let (start, end, flags) = if self.tp.snd_una == self.iss {
            (
                self.iss,
                self.iss
                    .wrapping_add(self.tp.snd_max.wrapping_sub(self.tp.snd_una)),
                RACK_HAS_SYN,
            )
        } else {
            (self.tp.snd_una, self.tp.snd_max, 0)
        };
        if let Some(map) = self.sendmaps.get_mut(&id) {
            map.start = start;
            map.end = end;
            map.flags = flags;
            map.rtr_cnt = 1;
            map.no_rtt_allowed = true;
            map.in_tmap = true;
        }
        Ok(id)
    }

    /// FreeBSD `rack_init`.
    pub fn rack_init(&mut self) -> Result<(), Errno> {
        self.initialized = true;
        self.alloc_limit_reported = false;
        self.rack_init_retransmit_value(0);
        self.rack_init_fsb()?;
        if self.tp.t_state != TCPS_CLOSED && self.tp.t_state != 6 {
            self.rack_deferred_init();
        }
        Ok(())
    }

    /// FreeBSD `rack_fini`.
    pub fn rack_fini(&mut self) -> usize {
        if !self.initialized {
            return 0;
        }
        let mut freed = 0;
        if let Some(mbuf) = self.fsb.mbuf.take() {
            self.os.m_freem(Some(mbuf));
        }
        for id in self.sendmaps.keys().copied().collect::<Vec<_>>() {
            self.sendmaps.remove(&id);
            let _ = self.os.uma_zfree(self.sendmap_zone, id);
            freed += 1;
        }
        self.free.clear();
        self.free_count = 0;
        self.maps_alloced = 0;
        self.split_allocs = 0;
        self.deferred.clear();
        self.fsb_inited = false;
        self.deferred_inited = false;
        self.initialized = false;
        freed
    }

    /// FreeBSD `rack_mtu_change`.
    pub fn rack_mtu_change(&mut self) -> bool {
        if self.min_segs == self.tp.t_maxseg {
            return false;
        }
        for map in self.sendmaps.values_mut() {
            map.flags |= RACK_MUST_RXT | RACK_PMTU_CHG;
        }
        self.tp.snd_nxt = self.tp.snd_max;
        true
    }

    /// FreeBSD `rack_sndbuf_autoscale`.
    pub fn rack_sndbuf_autoscale(&mut self) -> bool {
        let sendwin = self.snd_cwnd.min(self.snd_wnd);
        let used = self.os.snd.ccc;
        let hiwat = self.os.snd.hiwat;
        if !self.sndbuf_autosize
            || (self.snd_wnd / 4 * 5) < hiwat
            || used < hiwat / 8 * 7
            || used >= self.autosndbuf_max
            || sendwin < used.saturating_sub(self.tp.snd_max.wrapping_sub(self.tp.snd_una))
        {
            return false;
        }
        let scaleup = (self.autosndbuf_inc * hiwat / 100)
            .max(self.autosndbuf_inc)
            .saturating_add(hiwat)
            .min(self.autosndbuf_max);
        if self.os.sbreserve_locked(scaleup) {
            self.os.snd.hiwat = scaleup;
            true
        } else {
            self.sndbuf_autosize = false;
            false
        }
    }

    /// FreeBSD `rack_set_profile`.
    pub fn rack_set_profile(&mut self, profile: i32) -> Result<(), Errno> {
        match profile {
            1 | 6 => {
                self.dgp_on = true;
                self.always_pace = true;
                self.pace_to_cwnd = profile == 6;
                self.full_size_rxt = true;
                self.shape_rxt_to_pacing_min = false;
                self.use_cmp_ack = true;
                self.mbuf_queue = true;
                self.profile = profile;
                Ok(())
            }
            0 => {
                self.dgp_on = false;
                self.always_pace = false;
                self.pace_to_cwnd = false;
                self.full_size_rxt = false;
                self.shape_rxt_to_pacing_min = false;
                self.use_cmp_ack = false;
                self.mbuf_queue = false;
                self.profile = 0;
                Ok(())
            }
            _ => Err(Errno::EINVAL),
        }
    }

    /// FreeBSD `rack_add_deferred_option`.
    pub fn rack_add_deferred_option(&mut self, name: i32, value: u64) -> bool {
        if self.deferred.len() >= RACK_DEFERRED_MAX {
            return false;
        }
        self.deferred.push_back(DeferredOption { name, value });
        true
    }

    /// FreeBSD `rack_stack_information`.
    #[must_use]
    pub fn rack_stack_information(&self) -> RackStackInfo {
        RackStackInfo {
            bytes_transmitted: self.bytes_transmitted,
            bytes_retransmitted: self.bytes_retransmitted,
        }
    }

    /// FreeBSD `rack_process_option` for the K334 option subset.
    pub fn rack_process_option(&mut self, name: i32, value: u64) -> Result<(), Errno> {
        let value32 = u32::try_from(value & 0xffff_ffff).unwrap_or(u32::MAX);
        match name {
            TCP_RACK_PROFILE => self.rack_set_profile(i32::try_from(value).unwrap_or(0))?,
            TCP_DEFER_OPTIONS => self.defer_options = value != 0,
            TCP_RACK_SET_RXT_OPTIONS if value <= 2 => {
                self.rack_init_retransmit_value(i32::try_from(value).unwrap_or(0));
            }
            TCP_RACK_SET_RXT_OPTIONS => return Err(Errno::EINVAL),
            TCP_RACK_DSACK_OPT => {
                self.dsack_std_based = value & 1 != 0;
                self.dsack_use = value & 2 != 0;
            }
            TCP_RACK_HI_BETA => self.hi_beta = value != 0,
            TCP_RACK_TIMER_SLOP => self.timer_slop = value32,
            TCP_RACK_MEASURE_CNT => self.req_measurements = value32.max(1),
            TCP_NO_PRR => self.no_prr = value != 0,
            TCP_USE_CMP_ACKS => self.use_cmp_ack = value != 0,
            TCP_RACK_MBUF_QUEUE => self.mbuf_queue = value != 0,
            TCP_RACK_PACE_TO_FILL => self.pace_to_cwnd = value != 0,
            TCP_RACK_FORCE_MSEG => self.force_max_seg = value != 0,
            TCP_RACK_ABC_VAL => self.abc_val = u8::try_from(value).unwrap_or(u8::MAX),
            TCP_RACK_PACING_DIVISOR => {
                self.pacing_divisor = if value32 == 0 {
                    crate::RACK_DEFAULT_PACING_DIVISOR
                } else {
                    value32.max(crate::RL_MIN_DIVISOR)
                };
            }
            TCP_RACK_PACE_MIN_SEG => {
                self.min_segs = u16::try_from(value).unwrap_or(u16::MAX);
            }
            TCP_RACK_PACE_MAX_SEG => {
                self.max_segs = u16::try_from(value.min(u64::from(crate::MAX_USER_SET_SEG)))
                    .unwrap_or(u16::MAX);
            }
            TCP_PACING_DND => self.pacing_dnd = value != 0,
            _ => return Err(Errno::EOPNOTSUPP),
        }
        Ok(())
    }

    /// FreeBSD `rack_inherit`.
    pub fn rack_inherit(&mut self, parent: &Self) -> u32 {
        let mut count = 0;
        macro_rules! inherit {
            ($field:ident) => {
                if self.$field != parent.$field {
                    self.$field = parent.$field;
                    count += 1;
                }
            };
        }
        inherit!(full_size_rxt);
        inherit!(shape_rxt_to_pacing_min);
        inherit!(dsack_std_based);
        inherit!(dsack_use);
        inherit!(pacing_divisor);
        inherit!(hi_beta);
        inherit!(timer_slop);
        inherit!(req_measurements);
        inherit!(pacing_dnd);
        inherit!(no_prr);
        inherit!(use_cmp_ack);
        inherit!(mbuf_queue);
        inherit!(force_max_seg);
        inherit!(min_segs);
        inherit!(max_segs);
        inherit!(dgp_on);
        if parent.dgp_on && !self.dgp_on {
            self.dgp_on = true;
            count += 1;
        }
        count
    }

    /// FreeBSD `rack_apply_deferred_options`.
    pub fn rack_apply_deferred_options(&mut self) -> usize {
        let pending: Vec<DeferredOption> = self.deferred.drain(..).collect();
        let mut applied = 0;
        for option in pending {
            if self.rack_process_option(option.name, option.value).is_ok() {
                applied += 1;
            }
        }
        applied
    }

    /// FreeBSD `rack_set_sockopt`.
    pub fn rack_set_sockopt(&mut self, option: &RackSockopt) -> Result<(), Errno> {
        if option.dir != SockoptDir::Set || option.level != IPPROTO_TCP {
            return Err(Errno::EOPNOTSUPP);
        }
        if self.defer_options
            && option.name != TCP_DEFER_OPTIONS
            && option.name != TCP_RACK_SET_RXT_OPTIONS
            && option.name != TCP_RACK_MEASURE_CNT
        {
            return if self.rack_add_deferred_option(option.name, u64::from(option.value)) {
                Ok(())
            } else {
                Err(Errno::ENOMEM)
            };
        }
        self.rack_process_option(option.name, u64::from(option.value))
    }

    /// FreeBSD `rack_fill_info`.
    #[must_use]
    pub fn rack_fill_info(&self) -> RackTcpInfo {
        let mut info = RackTcpInfo {
            state: self.tp.t_state,
            options: 0,
            rtt: self.tp.t_srtt,
            rttvar: self.tp.t_rttvar,
            rto: self.tp.t_rxtcur,
            snd_ssthresh: self.tp.snd_ssthresh,
            snd_cwnd: self.tp.snd_cwnd,
            rcv_space: self.os.snd.hiwat,
            rcv_nxt: 0,
            snd_wnd: self.snd_wnd,
            snd_nxt: self.tp.snd_nxt,
            snd_mss: self.tp.t_maxseg,
            rcv_mss: self.tp.t_maxseg,
            snd_rexmitpack: 0,
            rcv_ooopack: 0,
            snd_zerowin: 0,
            total_tlp: self.tp.t_rxtshift.into(),
            total_tlp_bytes: 0,
            rttmin: self.tp.t_rttmin,
        };
        if self.tp.t_flags & crate::TF_SACK_PERMIT != 0 {
            info.options |= 1;
        }
        info
    }

    /// FreeBSD `rack_get_sockopt`.
    pub fn rack_get_sockopt(&self, option: &RackSockopt) -> Result<Option<u32>, Errno> {
        if option.dir != SockoptDir::Get || option.level != IPPROTO_TCP {
            return Err(Errno::EOPNOTSUPP);
        }
        let value = match option.name {
            TCP_INFO => return Err(Errno::EOPNOTSUPP),
            TCP_RACK_PROFILE => return Err(Errno::EINVAL),
            TCP_DEFER_OPTIONS => u32::from(self.defer_options),
            TCP_RACK_DSACK_OPT => {
                u32::from(self.dsack_std_based) | (u32::from(self.dsack_use) << 1)
            }
            TCP_RACK_HI_BETA => u32::from(self.hi_beta),
            TCP_RACK_TIMER_SLOP => self.timer_slop,
            TCP_RACK_MEASURE_CNT => self.req_measurements,
            TCP_NO_PRR => u32::from(self.no_prr),
            TCP_USE_CMP_ACKS => u32::from(self.use_cmp_ack),
            TCP_RACK_MBUF_QUEUE => u32::from(self.mbuf_queue),
            TCP_RACK_PACE_TO_FILL => u32::from(self.pace_to_cwnd),
            TCP_RACK_FORCE_MSEG => u32::from(self.force_max_seg),
            TCP_RACK_ABC_VAL => u32::from(self.abc_val),
            TCP_RACK_PACING_DIVISOR => self.pacing_divisor,
            TCP_RACK_PACE_MIN_SEG => u32::from(self.min_segs),
            TCP_RACK_PACE_MAX_SEG => u32::from(self.max_segs),
            TCP_PACING_DND => u32::from(self.pacing_dnd),
            TCP_RACK_PACING_BETA => {
                if self.full_size_rxt {
                    50
                } else {
                    0
                }
            }
            _ => return Err(Errno::EOPNOTSUPP),
        };
        Ok(Some(value))
    }

    /// FreeBSD `rack_ctor`.
    pub fn rack_ctor(memory: &mut [u8]) -> i32 {
        memory.fill(0);
        0
    }

    /// FreeBSD `rack_dtor`.
    pub fn rack_dtor(_memory: &mut [u8]) {}

    /// FreeBSD `rack_chg_query`.
    pub fn rack_chg_query(&self, query: &mut MiscQuery) -> i32 {
        match query.req {
            TCP_QUERY_SENDMAP => {
                if query.req_param == self.tp.snd_max || self.tp.snd_max == self.tp.snd_una {
                    return 0;
                }
                let Some(map) = self
                    .sendmaps
                    .values()
                    .find(|map| map.start <= query.req_param && query.req_param < map.end)
                else {
                    return 0;
                };
                query.sendmap_start = map.start;
                query.sendmap_end = map.end;
                query.sendmap_send_cnt = map.rtr_cnt;
                query.sendmap_fas = map.fas;
                query.sendmap_time = map.tim_lastsent;
                query.sendmap_flags = map.flags;
                query.sendmap_r_rtr_bytes = map.rtr_bytes;
                query.sendmap_dupacks = map.dupack;
                1
            }
            TCP_QUERY_TIMERS_UP => {
                if self.os.pacing.method == crate::PacingMethod::None {
                    return 0;
                }
                query.timer_hpts_flags = 1;
                query.timer_pacing_to = self.os.now_us;
                query.timer_timer_exp = self.os.now_us;
                1
            }
            TCP_QUERY_RACK_TIMES => {
                query.rack_num_dsacks = 0;
                query.rack_reorder_ts = 0;
                query.rack_rxt_last_time = 0;
                query.rack_min_rtt = 0;
                query.rack_rtt = self.tp.t_srtt;
                query.rack_tmit_time = 0;
                query.rack_srtt_measured = u32::from(self.tp.t_srtt != 0);
                1
            }
            _ => -22,
        }
    }

    /// FreeBSD `rack_pru_options`.
    pub fn rack_pru_options(flags: i32) -> Result<(), Errno> {
        if flags & PRUS_OOB != 0 {
            Err(Errno::EOPNOTSUPP)
        } else {
            Ok(())
        }
    }

    /// FreeBSD `rc_init_window`: the RFC 6928 initial window for the MSS.
    #[must_use]
    pub fn rc_init_window(&self) -> u32 {
        let mss = u32::from(self.tp.t_maxseg.max(1));
        (10 * mss).min((2 * mss).max(14_600))
    }

    /// FreeBSD `rack_swap_beta_values`: exchange the saved beta with the CC.
    pub fn rack_swap_beta_values(&mut self, beta: u32, beta_ecn: u32) -> bool {
        self.pacing_cc_set = true;
        if !self.cc_is_newreno {
            return false;
        }
        self.saved_beta = beta;
        self.saved_beta_ecn = beta_ecn;
        true
    }

    /// FreeBSD `rack_validate_multipliers_at_or_above100`.
    pub fn rack_validate_multipliers_at_or_above100(&mut self) -> u32 {
        let mut changed = 0;
        for percent in [
            &mut self.per_gp_rec,
            &mut self.per_gp_ca,
            &mut self.per_gp_ss,
        ] {
            if *percent < 100 {
                *percent = 100;
                changed += 1;
            }
        }
        changed
    }

    /// FreeBSD `rack_validate_multipliers_at_or_below_100`.
    pub fn rack_validate_multipliers_at_or_below_100(&mut self) -> u32 {
        let mut changed = 0;
        for percent in [&mut self.per_gp_ca, &mut self.per_gp_ss] {
            if *percent > 100 {
                *percent = 100;
                changed += 1;
            }
        }
        changed
    }

    /// FreeBSD `rack_update_multiplier`: one measurement-driven multiplier step.
    pub fn rack_update_multiplier(
        &mut self,
        timely_says: i32,
        last_bw_est: u64,
        _rtt: u32,
        _rtt_diff: i32,
    ) -> i32 {
        if !self.gp_dyn_mul || self.use_fixed_rate || self.in_probe_rtt || !self.always_pace {
            return 0;
        }
        let losses = self.loss_count.saturating_sub(self.loss_at_start);
        if losses > 0 {
            return 0;
        }
        if last_bw_est > self.last_gp_comp_bw {
            self.last_gp_comp_bw = last_bw_est;
            self.gp_bwred = false;
            self.gp_timely_dec_cnt = 0;
            1
        } else if last_bw_est < self.last_gp_comp_bw {
            if timely_says == 2 && self.no_push_at_mrtt > 1 {
                self.rack_validate_multipliers_at_or_below_100();
            }
            self.gp_bwred = true;
            if self.gp_timely_dec_cnt < 6 {
                self.gp_timely_dec_cnt += 1;
                2
            } else {
                -1
            }
        } else {
            0
        }
    }

    /// FreeBSD `tcp_rack_partialack`.
    pub fn tcp_rack_partialack(&mut self) -> bool {
        if self.prr_sndcnt > 0 || self.no_prr {
            self.wanted_output = true;
        }
        self.wanted_output
    }

    /// FreeBSD `rack_cc_after_idle`.
    pub fn rack_cc_after_idle(&mut self) {
        let iw = if self.tp.snd_cwnd == 1 {
            u32::from(self.tp.t_maxseg)
        } else {
            self.rc_init_window()
        };
        if self.tp.snd_cwnd < iw {
            self.tp.snd_cwnd = iw;
        }
    }

    /// FreeBSD `rack_adjust_orig_mlen`.
    pub fn rack_adjust_orig_mlen(&mut self, id: u64) -> bool {
        let Some(map) = self.sendmaps.get_mut(&id) else {
            return false;
        };
        if map.orig_t_space > 0 {
            map.soff = map.soff.saturating_sub(map.orig_t_space);
            map.orig_m_len = map.orig_m_len.saturating_add(map.orig_t_space);
            map.orig_t_space = 0;
            return true;
        }
        false
    }

    /// FreeBSD `rack_setup_offset_for_rsm`.
    pub fn rack_setup_offset_for_rsm(&mut self, src: u64, dst: u64) -> bool {
        let Some(source) = self.sendmaps.get(&src) else {
            return false;
        };
        let soff = source
            .soff
            .saturating_add(source.end.saturating_sub(source.start));
        let orig = source.orig_m_len;
        let tspace = source.orig_t_space;
        let Some(target) = self.sendmaps.get_mut(&dst) else {
            return false;
        };
        target.soff = soff;
        target.orig_m_len = orig;
        target.orig_t_space = tspace;
        true
    }

    /// FreeBSD `rack_send_ack_challange`.
    pub fn rack_send_ack_challange(&mut self) -> bool {
        if !self.forced_ack {
            self.forced_ack = true;
            self.forced_ack_ts = self.os.tcp_get_usecs().max(1);
        } else {
            self.probe_not_answered = true;
        }
        self.tp.t_flags &= !0x0020; // TF_DELACK
        true
    }

    /// FreeBSD `rack_convert_rtts`.
    pub fn rack_convert_rtts(&mut self) {
        let rto = self
            .tp
            .t_srtt
            .saturating_add(4u32.saturating_mul(self.tp.t_rttvar.max(1)))
            .clamp(30_000, 4_000_000);
        self.tp.t_rxtcur = rto;
    }

    /// FreeBSD `rack_cc_conn_init`.
    pub fn rack_cc_conn_init(&mut self) {
        if self.tp.snd_ssthresh < self.snd_wnd {
            self.tp.snd_ssthresh = self.snd_wnd;
        }
        let iw = self.rc_init_window();
        if iw < self.tp.snd_cwnd {
            self.tp.snd_cwnd = iw;
        }
    }

    /// FreeBSD `tcp_rack_xmit_timer`.
    pub fn tcp_rack_xmit_timer(
        &mut self,
        rtt: i32,
        _len: u32,
        us_rtt: u32,
        confidence: u8,
        _rsm: Option<u64>,
        rtrcnt: u16,
    ) -> u32 {
        let rtt_u = u32::try_from(rtt.max(1)).unwrap_or(u32::MAX);
        self.rtt_lowest = self.rtt_lowest.min(rtt_u);
        self.rtt_highest = self.rtt_highest.max(rtt_u);
        if (self.best_us_rtt == u32::MAX || us_rtt < self.best_us_rtt)
            && (self.rtt_confidence == 0 || confidence != 0)
        {
            self.best_us_rtt = us_rtt;
            self.rtt_confidence = confidence;
            self.best_rtr_cnt = rtrcnt;
        }
        self.rtt_valid = true;
        self.rtt_tot = self.rtt_tot.saturating_add(u64::from(rtt_u));
        self.rtt_cnt = self.rtt_cnt.saturating_add(1);
        self.best_us_rtt
    }

    /// FreeBSD `tcp_rack_xmit_timer_commit`.
    pub fn tcp_rack_xmit_timer_commit(&mut self) -> u32 {
        if !self.rtt_valid || self.rtt_cnt == 0 {
            return 0;
        }
        let rtt = match self.rate_sample_method {
            0 => self.rtt_lowest,
            1 => self.rtt_highest,
            _ => u32::try_from(self.rtt_tot / u64::from(self.rtt_cnt)).unwrap_or(1),
        }
        .max(1);
        self.tp.t_srtt = if self.tp.t_srtt == 0 {
            rtt
        } else {
            (self.tp.t_srtt.saturating_mul(7).saturating_add(rtt)) / 8
        };
        let delta = self.tp.t_srtt.abs_diff(rtt);
        self.tp.t_rttvar = if self.tp.t_rttvar == 0 {
            rtt / 2
        } else {
            (self.tp.t_rttvar.saturating_mul(7).saturating_add(delta)) / 8
        }
        .max(1);
        self.tp.t_rxtcur = self
            .tp
            .t_srtt
            .saturating_add(4u32.saturating_mul(self.tp.t_rttvar))
            .clamp(30_000, 4_000_000);
        rtt
    }

    /// FreeBSD `rack_apply_updated_usrtt`.
    pub fn rack_apply_updated_usrtt(&mut self, us_rtt: u32, us_cts: u32) -> bool {
        if us_rtt < self.min_rtt_us {
            self.min_rtt_us = us_rtt;
            self.lower_rtt_us_cts = us_cts;
            true
        } else {
            false
        }
    }

    /// FreeBSD `is_rsm_inside_declared_tlp_block`.
    #[must_use]
    pub fn is_rsm_inside_declared_tlp_block(&self, start: u32, end: u32) -> bool {
        end >= self.last_tlp_acked_start && start <= self.last_tlp_acked_end
    }

    /// FreeBSD `rack_rsm_sender_update`.
    pub fn rack_rsm_sender_update(&mut self, id: u64) -> bool {
        if !self.gput_inprog || self.app_limited_needs_set {
            return false;
        }
        let Some(map) = self.sendmaps.get(&id) else {
            return false;
        };
        let end = map.end;
        let ts = map.last_sent();
        if end > self.gput_ack {
            self.gput_ack = end;
        }
        if ts > self.gp_cumack_ts {
            self.gp_cumack_ts = ts;
            return true;
        }
        false
    }

    /// FreeBSD `rack_check_bottom_drag`.
    pub fn rack_check_bottom_drag(&mut self, sbavail: u32, flight: u32) -> bool {
        let mss = u32::from(self.tp.t_maxseg.max(1));
        if self.tp.snd_max == self.tp.snd_una
            || (sbavail > 4 * mss && flight <= 4 * mss && self.snd_wnd > 2 * mss)
        {
            self.rack_validate_multipliers_at_or_above100();
            self.dragged_bottom = true;
            return true;
        }
        false
    }

    /// FreeBSD `rack_adjust_sendmap_head`.
    pub fn rack_adjust_sendmap_head(&mut self) -> usize {
        let mut adjusted = 0;
        for map in self.sendmaps.values_mut() {
            if map.start <= self.tp.snd_una && self.tp.snd_una < map.end {
                map.soff = 0;
                map.orig_m_len = map.end.saturating_sub(self.tp.snd_una);
                adjusted += 1;
            }
        }
        adjusted
    }

    /// FreeBSD `rack_req_check_for_comp`.
    pub fn rack_req_check_for_comp(&mut self, ack: u32) -> usize {
        let before = self.reqs.len();
        self.reqs.retain(|(_, end)| *end > ack);
        before.saturating_sub(self.reqs.len())
    }

    /// FreeBSD `rack_collapsed_window`.
    pub fn rack_collapsed_window(&mut self, _out: u32, ack: u32) {
        self.last_collapse_point = ack.wrapping_add(self.snd_wnd);
        self.high_collapse_point = self.tp.snd_max;
        self.has_collapsed = true;
        self.collapse_point_valid = true;
    }

    /// FreeBSD `rack_un_collapse_window`.
    pub fn rack_un_collapse_window(&mut self) -> usize {
        self.has_collapsed = false;
        let point = self.last_collapse_point;
        let mut count = 0;
        for map in self.sendmaps.values_mut() {
            if map.end > point {
                map.flags |= RACK_RWND_COLLAPSED;
                count += 1;
            }
        }
        count
    }

    /// FreeBSD `rack_validate_fo_sendwin_up`.
    pub fn rack_validate_fo_sendwin_up(&mut self, out: u32) -> bool {
        if !self.fast_output {
            return false;
        }
        if out.saturating_add(self.fsb_left_to_send) > self.snd_wnd {
            if out >= self.snd_wnd {
                self.fast_output = false;
            } else {
                self.fsb_left_to_send = self.snd_wnd - out;
                if self.fsb_left_to_send < u32::from(self.tp.t_maxseg) {
                    self.fast_output = false;
                }
            }
        }
        self.fast_output
    }

    /// FreeBSD `rack_check_data_after_close`.
    pub fn rack_check_data_after_close(&mut self, tlen: u32) -> bool {
        if tlen > 0 && !self.allow_data_after_close {
            self.tp.t_state = TCPS_CLOSED;
            return true;
        }
        false
    }

    /// FreeBSD `rack_new_round_starts`.
    pub fn rack_new_round_starts(&mut self) -> u32 {
        self.current_round = self.current_round.saturating_add(1);
        self.roundends = self.roundends.saturating_add(1);
        self.current_round
    }

    /// FreeBSD `rack_new_round_setup`.
    pub fn rack_new_round_setup(&mut self, ack: u32) -> u32 {
        self.last_round_ack = ack;
        self.current_round
    }

    /// FreeBSD `rack_fo_base_copym`.
    pub fn rack_fo_base_copym(&mut self, len: u32) -> Option<MbufId> {
        let id = self.os.m_getcl(0)?;
        if len > 0 {
            let bytes = alloc::vec![0u8; usize::try_from(len).unwrap_or(0)];
            if !self.os.mbufs.write(id, 0, &bytes) || !self.os.mbufs.set_len(id, bytes.len()) {
                return None;
            }
        }
        Some(id)
    }

    /// FreeBSD `rack_fo_m_copym`.
    pub fn rack_fo_m_copym(&mut self, id: MbufId, off: u32, len: u32) -> Option<MbufId> {
        let off = usize::try_from(off).unwrap_or(0);
        let len = usize::try_from(len).unwrap_or(0);
        self.os.mbufs.m_copym(Some(id), off, len)
    }

    /// FreeBSD `rack_check_queue_level`.
    pub fn rack_check_queue_level(&mut self, level: u32) -> bool {
        self.high_rwnd = self.high_rwnd.max(level);
        level >= self.snd_cwnd.max(1)
    }

    /// FreeBSD `rack_fast_output`.
    pub fn rack_fast_output(&mut self) -> bool {
        if !self.fast_output || self.os.sbavail() == 0 {
            return false;
        }
        self.fsb_left_to_send = self.snd_wnd.min(self.snd_cwnd);
        true
    }

    /// FreeBSD `rack_setup_fast_output`.
    pub fn rack_setup_fast_output(&mut self) -> bool {
        self.fast_output_started = true;
        self.fast_output = self.snd_wnd > 0 && self.snd_cwnd > 0 && self.os.sbavail() > 0;
        if self.fast_output {
            self.fsb_left_to_send = self.snd_wnd.min(self.snd_cwnd);
            self.fsb.initialized = true;
        }
        self.fast_output
    }

    /// FreeBSD `rack_check_collapsed`.
    #[must_use]
    pub fn rack_check_collapsed(&self) -> bool {
        self.has_collapsed && self.collapse_point_valid
    }

    /// FreeBSD `rack_validate_sizes`.
    pub fn rack_validate_sizes(&self, len: u32, mss: u32) -> Result<(), Errno> {
        if len == 0 || mss == 0 || len > mss.saturating_mul(64) {
            return Err(Errno::EINVAL);
        }
        Ok(())
    }
}

/// `TH_ACK` made available to callers of this module.
pub const ACK_FLAG: u16 = TH_ACK;
/// `TH_FIN` made available to callers of this module.
pub const FIN_FLAG: u16 = TH_FIN;
/// `TH_RST` made available to callers of this module.
pub const RST_FLAG: u16 = TH_RST;

#[cfg(test)]
#[path = "rack_misc_ut.rs"]
mod tests;
