// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FreeBSD `sys/netinet/tcp_stacks/rack.c` BBR/pacing/timely/probe-RTT/measure/
//! rate/lt_bw/hybrid functions (K328).
//!
//! Donor: FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`, BSD-2-Clause.
//! The functions operate on the K314 `RackOs` pacing/log context and the K318
//! `RackStack` tcpcb mirror. `rack_set_cc_pacing`, `rack_undo_cc_pacing`,
//! `rack_remove_pacing` and `rack_get_pacing_len` are already ported by K314 and
//! are reused through `stack.os.pacing` instead of duplicated here.

use alloc::vec::Vec;

use crate::rack_os::RackOs;
use crate::rack_types::{PACE_MAX_IP_BYTES, RACK_INITIAL_RTO, TF_CONGRECOVERY, TF_FASTRECOVERY};
use crate::rack_vtable::{RackStack, TF2_MBUF_ACKCMP, TF2_SUPPORTS_MBUFQ};

/// FreeBSD `BBR_UNIT` (tcp_bbr.h).
pub const BBR_UNIT: u32 = 256;
/// FreeBSD `HPTS_USEC_IN_SEC`.
pub const HPTS_USEC_IN_SEC: u64 = 1_000_000;
/// FreeBSD `HPTS_USEC_IN_MSEC`.
pub const HPTS_USEC_IN_MSEC: u64 = 1_000;
/// FreeBSD `RACK_MIN_BW` (64 kbps in bytes/s).
pub const RACK_MIN_BW: u64 = 8_000;
/// FreeBSD `MIN_GP_WIN`: minimum MSS in a goodput measurement.
pub const MIN_GP_WIN: u32 = 6;
/// FreeBSD `RACK_REQ_AVG`.
pub const RACK_REQ_AVG: u32 = 3;
/// FreeBSD `RACK_TIMELY_CNT_BOOST`.
pub const RACK_TIMELY_CNT_BOOST: u32 = 5;
/// FreeBSD `TF_GPUTINPROG`.
pub const TF_GPUTINPROG: u32 = 0x0000_8000;
/// FreeBSD `RACK_DGP_PACING`.
pub const RACK_DGP_PACING: u32 = 0x01;
/// FreeBSD `RACK_REG_PACING`.
pub const RACK_REG_PACING: u32 = 0x02;
/// FreeBSD `RACK_QUALITY_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    /// `RACK_QUALITY_NONE`.
    None,
    /// `RACK_QUALITY_HIGH`.
    High,
    /// `RACK_QUALITY_APPLIMITED`.
    AppLimited,
    /// `RACK_QUALITY_PERSIST`.
    Persist,
    /// `RACK_QUALITY_PROBERTT`.
    ProbeRtt,
    /// `RACK_QUALITY_ALLACKED`.
    AllAcked,
}

/// FreeBSD `TCP_HYBRID_PACING_*` flags.
pub const HYBRID_CU: u32 = 0x0001;
/// `TCP_HYBRID_PACING_DTL`.
#[allow(dead_code)]
pub const HYBRID_DTL: u32 = 0x0002;
/// `TCP_HYBRID_PACING_CSPR`.
pub const HYBRID_CSPR: u32 = 0x0004;
/// `TCP_HYBRID_PACING_H_MS`.
pub const HYBRID_H_MS: u32 = 0x0008;
/// `TCP_HYBRID_PACING_ENABLE`.
pub const HYBRID_ENABLE: u32 = 0x0010;
/// `TCP_HYBRID_PACING_S_MSS`.
#[allow(dead_code)]
pub const HYBRID_S_MSS: u32 = 0x0020;
/// `TCP_HYBRID_PACING_SETMSS`.
pub const HYBRID_SETMSS: u32 = 0x1000;
/// `TCP_HYBRID_PACING_WASSET`.
pub const HYBRID_WASSET: u32 = 0x2000;
/// `TCP_HYBRID_PACING_SENDTIME`.
pub const HYBRID_SENDTIME: u32 = 0x4000;
/// `TCP_TRK_TRACK_FLG_*`.
pub const TRK_EMPTY: u32 = 0;
/// `TCP_TRK_TRACK_FLG_USED`.
pub const TRK_USED: u32 = 0x01;
/// `TCP_TRK_TRACK_FLG_FSND`.
pub const TRK_FSND: u32 = 0x10;
/// `TCP_TRK_TRACK_FLG_LSND`.
#[allow(dead_code)]
pub const TRK_LSND: u32 = 0x20;

/// Default knobs used by the donor's sysctls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BbrTunables {
    /// `rack_def_data_window`.
    pub def_data_window: u32,
    /// `rack_goal_bdp`.
    pub goal_bdp: u32,
    /// `rack_min_measure_usec`.
    pub min_measure_usec: u32,
    /// `rack_gp_rtt_maxmul`.
    pub rtt_maxmul: u32,
    /// `rack_gp_rtt_minmul`.
    pub rtt_minmul: u32,
    /// `rack_gp_rtt_mindiv`.
    pub rtt_mindiv: u32,
    /// `rack_gp_increase_per`.
    pub increase_per: u32,
    /// `rack_gp_decrease_per`.
    pub decrease_per: u32,
    /// `rack_per_lower_bound`.
    pub per_lower_bound: u32,
    /// `rack_gain_p5_ub`.
    pub gain_p5_ub: u32,
    /// `rack_max_per_above`.
    pub max_per_above: u32,
    /// `rack_timely_min_segs`.
    pub timely_min_segs: u32,
    /// `rack_timely_dec_clear`.
    pub timely_dec_clear: u32,
    /// `rack_wma_divisor`.
    pub wma_divisor: u32,
    /// `rack_probertt_use_min_rtt_entry`.
    pub probertt_use_min_rtt_entry: bool,
    /// `rack_probertt_use_min_rtt_exit`.
    pub probertt_use_min_rtt_exit: u8,
    /// `rack_probe_rtt_sets_cwnd`.
    pub probe_rtt_sets_cwnd: u32,
    /// `rack_probe_rtt_safety_val`.
    pub probe_rtt_safety: u32,
    /// `rack_time_between_probertt`.
    pub time_between_probertt: u32,
    /// `rack_min_probertt_hold`.
    pub min_probertt_hold: u32,
    /// `rack_max_drain_wait`.
    pub max_drain_wait: u32,
    /// `rack_max_drain_hbp`.
    pub max_drain_hbp: u32,
    /// `rack_must_drain`.
    pub must_drain: u32,
    /// `rack_per_of_gp_probertt`.
    pub per_of_gp_probertt: u32,
    /// `rack_per_of_gp_lowthresh`.
    pub per_of_gp_lowthresh: u32,
    /// `rack_per_of_gp_probertt_reduce`.
    pub per_of_gp_probertt_reduce: u32,
    /// `rack_atexit_prtt`.
    pub atexit_prtt: u32,
    /// `rack_atexit_prtt_hbp`.
    pub atexit_prtt_hbp: u32,
    /// `rack_gain_p5_ub`.
    pub fillcw_bw_cap: u64,
    /// `rack_bw_multipler`.
    pub bw_multipler: u32,
    /// `rack_limit_time_with_srtt`.
    pub limit_time_with_srtt: bool,
    /// `rack_timely_no_stopping`.
    pub timely_no_stopping: bool,
}

impl Default for BbrTunables {
    fn default() -> Self {
        Self {
            def_data_window: 20,
            goal_bdp: 2,
            min_measure_usec: 0,
            rtt_maxmul: 3,
            rtt_minmul: 1,
            rtt_mindiv: 4,
            increase_per: 2,
            decrease_per: 80,
            per_lower_bound: 50,
            gain_p5_ub: 250,
            max_per_above: 30,
            timely_min_segs: 4,
            timely_dec_clear: 6,
            wma_divisor: 8,
            probertt_use_min_rtt_entry: true,
            probertt_use_min_rtt_exit: 0,
            probe_rtt_sets_cwnd: 0,
            probe_rtt_safety: 2_000_000,
            time_between_probertt: 96_000_000,
            min_probertt_hold: 200_000,
            max_drain_wait: 2,
            max_drain_hbp: 1,
            must_drain: 1,
            per_of_gp_probertt: 60,
            per_of_gp_lowthresh: 40,
            per_of_gp_probertt_reduce: 10,
            atexit_prtt: 130,
            atexit_prtt_hbp: 130,
            fillcw_bw_cap: 3_750_000,
            bw_multipler: 0,
            limit_time_with_srtt: false,
            timely_no_stopping: false,
        }
    }
}

/// A donor `tcp_sendfile_track` subset used by the hybrid functions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SendfileTrack {
    /// `flags`.
    pub flags: u32,
    /// `hybrid_flags`.
    pub hybrid_flags: u32,
    /// `start`.
    pub start: u32,
    /// `end`.
    pub end: u32,
    /// `start_seq`.
    pub start_seq: u32,
    /// `end_seq`.
    pub end_seq: u32,
    /// `cspr`.
    pub cspr: u64,
    /// `hint_maxseg`.
    pub hint_maxseg: u32,
    /// `timestamp`.
    pub timestamp: u64,
    /// `localtime`.
    pub localtime: u64,
    /// `deadline`.
    pub deadline: u64,
    /// `first_send`.
    pub first_send: u64,
    /// `sent_at_fs`.
    pub sent_at_fs: u64,
    /// `rxt_at_fs`.
    pub rxt_at_fs: u64,
}

/// A donor `tcp_hybrid_req`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HybridRequest {
    /// `req.start`.
    pub start: u32,
    /// `req.end`.
    pub end: u32,
    /// `cspr`.
    pub cspr: u64,
    /// `hint_maxseg`.
    pub hint_maxseg: u32,
    /// `hybrid_flags`.
    pub hybrid_flags: u32,
}

/// One structured BBR/pacing log record (the donor's `union tcp_log_stackspecific`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BbrLogEvent {
    /// Donor event kind (a compact local code, not the kernel enum value).
    pub kind: u8,
    /// Donor `__LINE__` / caller line.
    pub line: u32,
    /// Eight donor flex/log fields.
    pub fields: [u64; 8],
}

/// A goodput measurement input; the socket send-map details stay at the caller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpSample {
    /// `th_ack`.
    pub ack: u32,
    /// `tp->gput_seq`.
    pub gput_seq: u32,
    /// `tp->gput_ack`.
    pub gput_ack: u32,
    /// `tp->gput_ts` in microseconds.
    pub gput_ts: u64,
    /// `rc_gp_cumack_ts` in microseconds.
    pub cumack_ts: u64,
    /// `rc_gp_output_ts` in microseconds.
    pub output_ts: u64,
    /// `rc_gp_lowrtt`.
    pub lowrtt: u32,
    /// `rc_gp_high_rwnd`.
    pub high_rwnd: u32,
    /// `rc_gp_srtt`.
    pub srtt: u32,
    /// `rc_prev_gp_srtt`.
    pub prev_srtt: u32,
    /// `act_rcv_time`/now in microseconds.
    pub now: u64,
    /// `tp->snd_wnd`.
    pub snd_wnd: u32,
    /// `rack->rc_gp_filled`.
    pub filled: bool,
    /// `rack->measure_saw_probe_rtt`.
    pub saw_probe_rtt: bool,
}

/// The CBR/BBR rate state carried by `tcp_rack` (`r_ctl` plus the rate fields).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BbrRack {
    /// `rack->lt_bw_up`.
    pub lt_bw_up: bool,
    /// `rack->r_ctl.lt_bw_time`.
    pub lt_bw_time: u64,
    /// `rack->r_ctl.lt_bw_bytes`.
    pub lt_bw_bytes: u64,
    /// `rack->r_ctl.lt_seq`.
    pub lt_seq: u32,
    /// `rack->r_ctl.lt_timemark`.
    pub lt_timemark: u64,
    /// `rack->dis_lt_bw`.
    pub dis_lt_bw: bool,
    /// `rack->use_lesser_lt_bw`.
    pub use_lesser_lt_bw: bool,
    /// `rack->rc_gp_filled`.
    pub gp_filled: bool,
    /// `rack->r_ctl.gp_bw`.
    pub gp_bw: u64,
    /// `rack->r_ctl.num_measurements`.
    pub num_measurements: u32,
    /// `rack->r_ctl.init_rate`.
    pub init_rate: u64,
    /// `rack->r_ctl.req_measurements`.
    pub req_measurements: u32,
    /// `rack->r_ctl.rc_gp_srtt`.
    pub gp_srtt: u32,
    /// `rack->r_ctl.rc_prev_gp_srtt`.
    pub prev_gp_srtt: u32,
    /// `rack->r_ctl.rc_gp_min_rtt`.
    pub gp_min_rtt: u32,
    /// `rack->r_ctl.rc_gp_lowrtt`.
    pub gp_lowrtt: u32,
    /// `rack->r_ctl.rc_gp_high_rwnd`.
    pub gp_high_rwnd: u32,
    /// `rack->r_ctl.last_max_bw`.
    pub last_max_bw: u64,
    /// `rack->r_ctl.rc_rtt_diff`.
    pub rtt_diff: i32,
    /// `rack->rc_gp_saw_rec`.
    pub gp_saw_rec: bool,
    /// `rack->rc_gp_saw_ca`.
    pub gp_saw_ca: bool,
    /// `rack->rc_gp_saw_ss`.
    pub gp_saw_ss: bool,
    /// `rack->rc_gp_no_rec_chg`.
    pub gp_no_rec_chg: bool,
    /// `rack->rc_gp_incr`.
    pub gp_incr: bool,
    /// `rack->rc_gp_timely_inc_cnt`.
    pub gp_timely_inc_cnt: u32,
    /// `rack->rc_gp_timely_dec_cnt`.
    pub gp_timely_dec_cnt: u32,
    /// `rack->r_ctl.rack_per_of_gp_rec`.
    pub per_gp_rec: u32,
    /// `rack->r_ctl.rack_per_of_gp_ca`.
    pub per_gp_ca: u32,
    /// `rack->r_ctl.rack_per_of_gp_ss`.
    pub per_gp_ss: u32,
    /// `rack->r_ctl.rack_per_upper_bound_ca`.
    pub per_upper_bound_ca: u32,
    /// `rack->r_ctl.rack_per_upper_bound_ss`.
    pub per_upper_bound_ss: u32,
    /// `rack->rc_dragged_bottom`.
    pub dragged_bottom: bool,
    /// `rack->rc_skip_timely`.
    pub skip_timely: bool,
    /// `rack->rc_gp_dyn_mul`.
    pub gp_dyn_mul: bool,
    /// `rack->rc_gp_rtt_set`.
    pub gp_rtt_set: bool,
    /// `rack->app_limited_needs_set`.
    pub app_limited_needs_set: bool,
    /// `rack->measure_saw_probe_rtt`.
    pub measure_saw_probe_rtt: bool,
    /// `rack->gp_ready`.
    pub gp_ready: bool,
    /// `rack->rc_always_pace`.
    pub always_pace: bool,
    /// `rack->use_fixed_rate`.
    pub use_fixed_rate: bool,
    /// `rack->dgp_on`.
    pub dgp_on: bool,
    /// `rack->rc_hybrid_mode`.
    pub hybrid_mode: bool,
    /// `rack->rc_catch_up`.
    pub catch_up: bool,
    /// `rack->r_ctl.client_suggested_maxseg`.
    pub client_suggested_maxseg: u32,
    /// `rack->cspr_is_fcc`.
    pub cspr_is_fcc: bool,
    /// `rack->in_probe_rtt`.
    pub in_probe_rtt: bool,
    /// `rack->r_ctl.rc_time_probertt_entered`.
    pub probertt_entered: u32,
    /// `rack->r_ctl.rc_time_probertt_starts`.
    pub probertt_starts: u32,
    /// `rack->r_ctl.rc_lower_rtt_us_cts`.
    pub probertt_lower: u32,
    /// `rack->r_ctl.rc_time_of_last_probertt`.
    pub probertt_last: u32,
    /// `rack->r_ctl.rc_entry_gp_rtt`.
    pub entry_gp_rtt: u32,
    /// `rack->r_ctl.rc_target_probertt_flight`.
    pub target_probertt_flight: u32,
    /// `rack->r_ctl.rack_per_of_gp_probertt`.
    pub per_of_gp_probertt: u32,
    /// `rack->r_ctl.rc_pace_min_segs`.
    pub pace_min_segs: u32,
    /// `rack->r_ctl.rc_pace_max_segs`.
    pub pace_max_segs: u32,
    /// `rack->r_ctl.cwnd_to_use`.
    pub cwnd_to_use: u32,
    /// `rack->r_ctl.rc_last_us_rtt`.
    pub last_us_rtt: u32,
    /// `rack->r_via_fill_cw`.
    pub via_fill_cw: bool,
    /// `rack->r_ctl.fillcw_cap`.
    pub fillcw_cap: u64,
    /// `rack->rtt_limit_mul`.
    pub rtt_limit_mul: u32,
    /// `rack->rc_pace_to_cwnd`.
    pub pace_to_cwnd: bool,
    /// `rack->rc_pace_fill_if_rttin_range`.
    pub pace_fill_if_rttin_range: bool,
    /// `rack->r_ctl.rc_min_to`.
    pub min_to: u32,
    /// `rack->rc_ack_can_sendout_data`.
    pub ack_can_sendout_data: bool,
    /// `rack->r_ctl.bw_rate_cap`.
    pub bw_rate_cap: u64,
    /// `rack->r_ctl.last_hw_bw_req`.
    pub last_hw_bw_req: u64,
    /// `rack->r_rare_hw_rate` (optional hardware ceiling).
    pub hw_rate_cap: u64,
    /// `rack->r_ctl.pacing_method`.
    pub pacing_method: u32,
    /// `rack->rc_gp_incr` source state.
    pub gp_incr_seen: bool,
    /// `rack->r_ctl.rc_loss_count`.
    pub loss_count: u32,
    /// `rack->r_is_v6`.
    pub is_v6: bool,
    /// `rack->r_ctl.rc_loss_at_start`.
    pub loss_at_start: u32,
    /// `rack->rc_gp_filled` initial-window flag.
    pub initial_ss_comp: bool,
    /// `rack->r_ctl.last_gpest`.
    pub last_gpest: u64,
    /// `rack->r_ctl.current_round`.
    pub current_round: u32,
    /// `rack->r_ctl.last_rnd_of_gp_rise`.
    pub last_rnd_of_gp_rise: u32,
    /// `rack->r_ctl.gp_gain_req`.
    pub gp_gain_req: u32,
    /// `rack->r_ctl.use_gp_not_last`.
    pub use_gp_not_last: bool,
    /// `rack->r_ctl.rc_app_limited_cnt`.
    pub app_limited_cnt: u32,
    /// Hybrid request-track state.
    pub last_sft: Option<SendfileTrack>,
    /// `rack->r_ctl.last_tm_mark`.
    pub last_tm_mark: u64,
    /// `rack->r_ctl.rc_went_idle_time`.
    pub went_idle_time: u32,
    /// `rack->rc_highly_buffered`.
    pub highly_buffered: bool,
    /// `rack->rc_pacing_cc_set`.
    pub pacing_cc_set: bool,
    /// `rack->cc_beta_ecn` (the non-saved current value).
    pub cc_beta_ecn: u32,
    /// `rack->gput_seq` mirror.
    pub gput_seq: u32,
    /// `rack->gput_ack` mirror.
    pub gput_ack: u32,
    /// `rack->gput_ts` mirror.
    pub gput_ts: u64,
    /// `rack->rc_gp_output_ts`.
    pub gp_output_ts: u64,
    /// `rack->rc_gp_cumack_ts`.
    pub gp_cumack_ts: u64,
    /// Knobs.
    pub tun: BbrTunables,
    /// Structured logs (bounded).
    pub log: Vec<BbrLogEvent>,
}

impl Default for BbrRack {
    fn default() -> Self {
        Self {
            lt_bw_up: false,
            lt_bw_time: 0,
            lt_bw_bytes: 0,
            lt_seq: 0,
            lt_timemark: 0,
            dis_lt_bw: false,
            use_lesser_lt_bw: false,
            gp_filled: false,
            gp_bw: 0,
            num_measurements: 0,
            init_rate: 0,
            req_measurements: RACK_REQ_AVG,
            gp_srtt: 0,
            prev_gp_srtt: 0,
            gp_min_rtt: u32::MAX,
            gp_lowrtt: u32::MAX,
            gp_high_rwnd: 0,
            last_max_bw: 0,
            rtt_diff: 0,
            gp_saw_rec: false,
            gp_saw_ca: false,
            gp_saw_ss: false,
            gp_no_rec_chg: false,
            gp_incr: false,
            gp_timely_inc_cnt: 0,
            gp_timely_dec_cnt: 0,
            per_gp_rec: 100,
            per_gp_ca: 100,
            per_gp_ss: 200,
            per_upper_bound_ca: 0,
            per_upper_bound_ss: 0,
            dragged_bottom: false,
            skip_timely: false,
            gp_dyn_mul: false,
            gp_rtt_set: false,
            app_limited_needs_set: false,
            measure_saw_probe_rtt: false,
            gp_ready: false,
            always_pace: false,
            use_fixed_rate: false,
            dgp_on: false,
            hybrid_mode: false,
            catch_up: false,
            client_suggested_maxseg: 0,
            cspr_is_fcc: false,
            in_probe_rtt: false,
            probertt_entered: 0,
            probertt_starts: 0,
            probertt_lower: 0,
            probertt_last: 0,
            entry_gp_rtt: 0,
            target_probertt_flight: 0,
            per_of_gp_probertt: 60,
            pace_min_segs: 0,
            pace_max_segs: 0,
            cwnd_to_use: 0,
            last_us_rtt: 0,
            via_fill_cw: false,
            fillcw_cap: 0,
            rtt_limit_mul: 0,
            pace_to_cwnd: false,
            pace_fill_if_rttin_range: false,
            min_to: 0,
            ack_can_sendout_data: false,
            bw_rate_cap: 0,
            last_hw_bw_req: 0,
            hw_rate_cap: 0,
            pacing_method: 0,
            gp_incr_seen: false,
            loss_count: 0,
            is_v6: false,
            loss_at_start: 0,
            initial_ss_comp: false,
            last_gpest: 0,
            current_round: 0,
            last_rnd_of_gp_rise: 0,
            gp_gain_req: 5,
            use_gp_not_last: false,
            app_limited_cnt: 0,
            last_sft: None,
            last_tm_mark: 0,
            went_idle_time: 0,
            highly_buffered: false,
            pacing_cc_set: false,
            cc_beta_ecn: 50,
            gput_seq: 0,
            gput_ack: 0,
            gput_ts: 0,
            gp_output_ts: 0,
            gp_cumack_ts: 0,
            tun: BbrTunables::default(),
            log: Vec::new(),
        }
    }
}

impl BbrRack {
    /// New state with donor defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn event(&mut self, kind: u8, line: u32, fields: [u64; 8]) {
        if self.log.len() >= 256 {
            self.log.remove(0);
        }
        self.log.push(BbrLogEvent { kind, line, fields });
    }

    #[must_use]
    fn maxseg(stack: &RackStack) -> u32 {
        u32::from(stack.tp.t_maxseg.max(1))
    }

    #[must_use]
    fn init_window(stack: &RackStack) -> u32 {
        // tcp_compute_initwnd(tcp_maxseg) = 10 * MSS for the donor default IW.
        Self::maxseg(stack).saturating_mul(10)
    }

    #[must_use]
    fn in_fast_recovery(stack: &RackStack) -> bool {
        stack.tp.t_flags & TF_FASTRECOVERY != 0
    }

    #[must_use]
    fn in_recovery(stack: &RackStack) -> bool {
        stack.tp.t_flags & (TF_FASTRECOVERY | TF_CONGRECOVERY) != 0
    }

    fn pace_len(stack: &RackStack, bw: u64, mss: u32) -> u32 {
        stack.os.pacing.rack_get_pacing_len(bw, mss)
    }

    /// FreeBSD `rack_get_lt_bw`.
    #[must_use]
    pub fn get_lt_bw(&self, stack: &RackStack, now_us: u64) -> u64 {
        let mut tim = self.lt_bw_time;
        let mut bytes = self.lt_bw_bytes;
        if self.lt_bw_up {
            bytes = bytes.saturating_add(u64::from(stack.tp.snd_una.wrapping_sub(self.lt_seq)));
            tim = tim.saturating_add(now_us.saturating_sub(self.lt_timemark));
        }
        if bytes != 0 && tim != 0 {
            bytes.saturating_mul(HPTS_USEC_IN_SEC) / tim
        } else {
            0
        }
    }

    /// FreeBSD `rack_log_gpset`.
    #[allow(clippy::too_many_arguments)] // Donor signature.
    fn log_gpset(
        &mut self,
        stack: &RackStack,
        seq_end: u32,
        ack_end: u32,
        send_end: u32,
        line: u32,
        mode: u8,
        rsm_start: u32,
        rsm_end: u32,
    ) {
        self.event(
            2,
            line,
            [
                u64::from(seq_end),
                u64::from(self.gput_seq),
                u64::from(ack_end),
                self.gput_ts,
                u64::from(send_end),
                u64::from(self.gput_ack),
                u64::from(mode),
                u64::from(rsm_start.wrapping_add(rsm_end).wrapping_add(stack.snd_wnd)),
            ],
        );
    }

    /// FreeBSD `rack_get_fixed_pacing_bw`.
    #[must_use]
    pub fn get_fixed_pacing_bw(&self, stack: &RackStack) -> u64 {
        if Self::in_fast_recovery(stack) {
            stack.options.pacing_rate_rec
        } else if self.cwnd_to_use < stack.tp.snd_ssthresh {
            stack.options.pacing_rate_ss
        } else {
            stack.options.pacing_rate_ca
        }
    }

    /// FreeBSD `rack_log_hybrid_bw`.
    #[allow(clippy::too_many_arguments)] // Donor signature.
    fn log_hybrid_bw(
        &mut self,
        stack: &RackStack,
        seq: u32,
        bw: u64,
        time: u64,
        len: u64,
        mode: u8,
        line: u32,
        track: Option<SendfileTrack>,
    ) {
        let ent = track.unwrap_or_default();
        self.event(
            3,
            line,
            [
                u64::from(seq),
                bw,
                time,
                len,
                u64::from(mode),
                u64::from(ent.start),
                u64::from(ent.end),
                u64::from(stack.tp.snd_max),
            ],
        );
    }

    /// FreeBSD `rack_compensate_for_linerate`.
    #[must_use]
    pub fn compensate_for_linerate(&self, stack: &RackStack, bw: u64) -> u64 {
        let ip = if self.is_v6 { 40 } else { 20 };
        let ether = u64::from(Self::maxseg(stack)) + 20 + ip + 14;
        let minimum = if self.pace_min_segs == 0 {
            Self::maxseg(stack)
        } else {
            self.pace_min_segs
        };
        let segsiz = Self::maxseg(stack).min(minimum.max(1));
        if segsiz == 0 {
            return bw;
        }
        bw.saturating_mul(ether) / u64::from(segsiz)
    }

    /// FreeBSD `rack_rate_cap_bw`; returns `(bw, capped)`.
    pub fn rate_cap_bw(&mut self, stack: &RackStack, bw: u64, now_us: u64) -> (u64, bool) {
        let mut bw = bw;
        let mut capped = false;
        if self.bw_rate_cap == 0 {
            return (bw, capped);
        }
        if self.catch_up
            && let Some(mut ent) = self.last_sft
        {
            if now_us >= ent.deadline || ent.deadline.saturating_sub(now_us) < 1000 {
                self.log_hybrid_bw(stack, stack.tp.snd_max, 0, 0, 0, 12, 0, Some(ent));
                self.bw_rate_cap = 0;
                return (bw, capped);
            }
            let timeleft = ent.deadline - now_us;
            let lenleft = ent.end_seq.saturating_sub(stack.tp.snd_una);
            if lenleft == 0 {
                self.log_hybrid_bw(stack, stack.tp.snd_max, 0, timeleft, 0, 11, 0, Some(ent));
            } else {
                let calcbw = self.compensate_for_linerate(
                    stack,
                    u64::from(lenleft) * HPTS_USEC_IN_SEC / timeleft,
                );
                self.bw_rate_cap = calcbw;
                self.log_hybrid_bw(
                    stack,
                    stack.tp.snd_max,
                    calcbw,
                    timeleft,
                    u64::from(lenleft),
                    9,
                    0,
                    Some(ent),
                );
                if calcbw > 0 && bw > calcbw {
                    capped = true;
                    bw = calcbw;
                }
                ent.hybrid_flags |= HYBRID_SETMSS;
                self.last_sft = Some(ent);
                return (bw, capped);
            }
        }
        if self.bw_rate_cap > 0 && bw > self.bw_rate_cap {
            capped = true;
            bw = self.bw_rate_cap;
            self.log_hybrid_bw(stack, stack.tp.snd_max, bw, 0, 0, 8, 0, None);
        }
        (bw, capped)
    }

    /// FreeBSD `rack_get_gp_est`.
    #[must_use]
    pub fn get_gp_est(&mut self, stack: &RackStack, now_us: u64) -> u64 {
        let ret_bw;
        if !self.gp_filled {
            let lt_bw = if self.dis_lt_bw {
                0
            } else {
                self.get_lt_bw(stack, now_us)
            };
            if lt_bw != 0 {
                ret_bw = lt_bw;
            } else if self.init_rate != 0 {
                return self.init_rate;
            } else if stack.tp.t_srtt == 0 {
                return 0;
            } else {
                ret_bw = u64::from(Self::init_window(stack)).saturating_mul(HPTS_USEC_IN_SEC)
                    / u64::from(stack.tp.t_srtt.max(1));
            }
        } else {
            let bw = if self.num_measurements >= RACK_REQ_AVG {
                self.gp_bw
            } else {
                self.gp_bw / u64::from(self.num_measurements.max(1))
            };
            if self.dis_lt_bw {
                ret_bw = bw;
            } else {
                let mut lt_bw = self.get_lt_bw(stack, now_us);
                if lt_bw == 0 {
                    lt_bw = self.gp_bw;
                }
                ret_bw = if self.use_lesser_lt_bw {
                    lt_bw.min(bw)
                } else {
                    lt_bw.max(bw)
                };
            }
        }
        self.compensate_for_linerate(stack, ret_bw)
    }

    /// FreeBSD `rack_get_bw`.
    #[must_use]
    pub fn get_bw(&mut self, stack: &RackStack, now_us: u64) -> u64 {
        if self.use_fixed_rate {
            self.get_fixed_pacing_bw(stack)
        } else {
            self.get_gp_est(stack, now_us)
        }
    }

    /// FreeBSD `rack_get_output_gain`.
    #[must_use]
    pub fn get_output_gain(&self, stack: &RackStack, retransmit: bool) -> u32 {
        if self.use_fixed_rate {
            100
        } else if self.in_probe_rtt && !retransmit {
            self.per_of_gp_probertt
        } else if Self::in_fast_recovery(stack) && self.per_gp_rec != 0 {
            if retransmit {
                self.per_gp_rec
            } else if stack.options.rec_nonrxt_use_cr {
                self.configured_rate(stack)
            } else if stack.options.no_prr && self.per_gp_rec > 100 {
                100
            } else {
                self.per_gp_rec
            }
        } else {
            self.configured_rate(stack)
        }
    }

    fn configured_rate(&self, stack: &RackStack) -> u32 {
        if self.cwnd_to_use < stack.tp.snd_ssthresh {
            self.per_gp_ss
        } else {
            self.per_gp_ca
        }
    }

    /// FreeBSD `rack_get_output_bw`.
    pub fn get_output_bw(&mut self, stack: &RackStack, bw: u64, retransmit: bool) -> (u64, bool) {
        let gain = u64::from(self.get_output_gain(stack, retransmit));
        let mut bw_est = bw.saturating_mul(gain) / 100;
        bw_est = bw_est.max(RACK_MIN_BW);
        let mut capped = false;
        if self.hw_rate_cap != 0 && bw_est >= self.hw_rate_cap {
            bw_est = self.hw_rate_cap;
            capped = true;
            self.log_hdwr_pacing(stack, bw_est, self.hw_rate_cap, 0, 0, 3);
        }
        (bw_est, capped)
    }

    /// FreeBSD `rack_log_hdwr_pacing`.
    fn log_hdwr_pacing(
        &mut self,
        stack: &RackStack,
        bw: u64,
        high: u64,
        err: i32,
        mode: u8,
        line: u32,
    ) {
        self.event(
            5,
            line,
            [
                bw,
                high,
                err as u64,
                u64::from(mode),
                u64::from(stack.tp.snd_max),
                0,
                0,
                0,
            ],
        );
    }

    /// FreeBSD `rack_log_type_pacing_sizes`.
    fn log_type_pacing_sizes(
        &mut self,
        _stack: &RackStack,
        orig_min: u32,
        orig_max: u32,
        line: u32,
        mode: u8,
    ) {
        self.event(
            7,
            line,
            [
                u64::from(orig_min),
                u64::from(orig_max),
                u64::from(self.pace_min_segs),
                u64::from(self.pace_max_segs),
                u64::from(mode),
                0,
                0,
                0,
            ],
        );
    }

    /// FreeBSD `rack_get_measure_window`.
    #[must_use]
    pub fn get_measure_window(&mut self, stack: &RackStack, now_us: u64) -> u32 {
        let minimum = if self.pace_min_segs == 0 {
            Self::maxseg(stack)
        } else {
            self.pace_min_segs
        };
        let segsiz = Self::maxseg(stack).min(minimum.max(1));
        let def_len = self.tun.def_data_window.saturating_mul(segsiz);
        if !self.gp_filled {
            return def_len;
        }
        let bw = self.get_bw(stack, now_us);
        let srtt = u64::from(stack.tp.t_srtt);
        let mut len = bw.saturating_mul(srtt) / HPTS_USEC_IN_SEC;
        len = len.saturating_mul(u64::from(self.tun.goal_bdp.max(1)));
        let round = |value: u64| -> u64 {
            let s = u64::from(segsiz.max(1));
            value.div_ceil(s).saturating_mul(s)
        };
        len = round(len);
        if self.tun.min_measure_usec != 0 {
            let mut minl =
                u64::from(self.tun.min_measure_usec).saturating_mul(bw) / HPTS_USEC_IN_SEC;
            if minl == 0 {
                minl = 1;
            }
            minl = round(minl);
            len = len.max(minl);
        }
        if len < u64::from(def_len) {
            (len as u32).max((MIN_GP_WIN + 2).saturating_mul(segsiz))
        } else {
            (len as u32).max(def_len)
        }
    }

    /// FreeBSD `rack_enough_for_measurement`.
    #[must_use]
    pub fn enough_for_measurement(
        &self,
        stack: &RackStack,
        ack: u32,
        now_us: u64,
    ) -> Option<Quality> {
        if ack < self.gput_seq {
            return None;
        }
        if stack.tp.snd_max == stack.tp.snd_una || ack == stack.tp.snd_max {
            return Some(Quality::AllAcked);
        }
        if ack >= self.gput_ack {
            return Some(Quality::High);
        }
        let segsiz = Self::maxseg(stack).min(self.pace_min_segs.max(1));
        if ack.wrapping_sub(self.gput_seq)
            < Self::init_window(stack).max(MIN_GP_WIN.saturating_mul(segsiz))
        {
            return None;
        }
        if self.app_limited_cnt != 0 && ack >= self.gput_seq.wrapping_add(Self::init_window(stack))
        {
            return Some(Quality::AppLimited);
        }
        let srtts = self.gp_srtt.saturating_mul(3);
        let tim = now_us.saturating_sub(self.gput_ts) as u32;
        if tim >= srtts && !Self::in_recovery(stack) {
            Some(Quality::High)
        } else {
            None
        }
    }

    /// FreeBSD `rack_log_timely`.
    fn log_timely(&mut self, line: u32, mode: u8, fields: [u64; 4]) {
        self.event(
            8,
            line,
            [
                u64::from(mode),
                fields[0],
                fields[1],
                fields[2],
                fields[3],
                0,
                0,
                0,
            ],
        );
    }

    /// FreeBSD `rack_bw_can_be_raised`.
    pub fn bw_can_be_raised(
        &mut self,
        stack: &RackStack,
        cur_bw: u64,
        last_bw_est: u64,
        mult: u16,
    ) -> bool {
        if self.tun.timely_no_stopping {
            return true;
        }
        if cur_bw == 0 || last_bw_est == 0 || mult <= 100 {
            self.log_timely(0, 9, [u64::from(mult), cur_bw, 0, 0]);
            return true;
        }
        let act_rate = cur_bw.saturating_mul(u64::from(mult)) / 100;
        let max_allow = last_bw_est.saturating_mul(u64::from(self.tun.max_per_above) + 100) / 100;
        let allowed = act_rate < max_allow;
        self.log_timely(
            0,
            if allowed { 9 } else { 8 },
            [u64::from(mult), act_rate, max_allow, 0],
        );
        let _ = stack;
        allowed
    }

    /// FreeBSD `rack_increase_bw_mul`.
    pub fn increase_bw_mul(
        &mut self,
        stack: &RackStack,
        timely_says: i32,
        cur_bw: u64,
        last_bw_est: u64,
        override_boost: bool,
    ) {
        if self.skip_timely {
            return;
        }
        let plus = if override_boost
            || (self.gp_incr && self.gp_timely_inc_cnt + 1 >= RACK_TIMELY_CNT_BOOST)
        {
            self.gp_timely_inc_cnt = 0;
            self.tun.increase_per.saturating_mul(RACK_TIMELY_CNT_BOOST)
        } else {
            self.tun.increase_per
        };
        let plus = if plus < 1 && (self.rtt_diff <= 0 || timely_says <= 0) {
            1
        } else {
            plus
        };
        let mut logged = 0u32;
        if self.gp_saw_rec
            && !self.gp_no_rec_chg
            && self.bw_can_be_raised(stack, cur_bw, last_bw_est, self.per_gp_rec as u16)
        {
            self.per_gp_rec = (self.per_gp_rec + plus).min(0xffff);
            if self.per_upper_bound_ca != 0 && !self.dragged_bottom {
                self.per_gp_rec = self.per_gp_rec.min(self.per_upper_bound_ca);
            }
            logged |= 1;
        }
        if self.gp_saw_ca
            && !self.gp_saw_ss
            && self.bw_can_be_raised(stack, cur_bw, last_bw_est, self.per_gp_ca as u16)
        {
            self.per_gp_ca = (self.per_gp_ca + plus).min(0xffff);
            if self.per_upper_bound_ca != 0 && !self.dragged_bottom {
                self.per_gp_ca = self.per_gp_ca.min(self.per_upper_bound_ca);
            }
            logged |= 2;
        }
        if self.gp_saw_ss
            && self.bw_can_be_raised(stack, cur_bw, last_bw_est, self.per_gp_ss as u16)
        {
            self.per_gp_ss = (self.per_gp_ss + plus).min(0xffff);
            if self.per_upper_bound_ss != 0 && !self.dragged_bottom {
                self.per_gp_ss = self.per_gp_ss.min(self.per_upper_bound_ss);
            }
            logged |= 4;
        }
        if logged != 0 && !self.gp_incr {
            self.gp_incr = true;
            self.gp_timely_inc_cnt = 0;
        }
        if self.gp_incr && logged != 0 && self.gp_timely_inc_cnt < RACK_TIMELY_CNT_BOOST {
            self.gp_timely_inc_cnt += 1;
        }
        self.log_timely(0, 1, [u64::from(logged), u64::from(plus), 0, 0]);
    }

    fn get_decrease(&self, curper: u32, rtt_diff: i32) -> u32 {
        let minrtt = u64::from(self.gp_min_rtt.max(1));
        let diff = u64::from(rtt_diff.unsigned_abs());
        let mut perf = u64::from(curper)
            * (1_000_000
                - (u64::from(self.tun.decrease_per)
                    * 10_000
                    * (diff.saturating_mul(1_000_000) / minrtt)
                    / 1_000_000))
            / 1_000_000;
        if perf > u64::from(curper) {
            perf = u64::from(curper.saturating_sub(1));
        }
        perf as u32
    }

    fn decrease_highrtt(&mut self, curper: u32, rtt: u32) -> u32 {
        let highrtt = self.gp_min_rtt.saturating_mul(self.tun.rtt_maxmul);
        let perf = u64::from(curper)
            * (1_000_000
                - (u64::from(self.tun.decrease_per)
                    * (1_000_000 - u64::from(highrtt) * 1_000_000 / u64::from(rtt.max(1)))
                    / 100))
            / 1_000_000;
        let perf = perf as u32;
        self.log_timely(
            0,
            15,
            [
                u64::from(self.tun.decrease_per),
                u64::from(curper),
                u64::from(highrtt),
                u64::from(perf),
            ],
        );
        perf
    }

    /// FreeBSD `rack_decrease_bw_mul`.
    pub fn decrease_bw_mul(&mut self, timely_says: i32, rtt: u32, rtt_diff: i32) {
        if self.skip_timely {
            return;
        }
        if self.gp_incr {
            self.gp_incr = false;
            self.gp_timely_inc_cnt = 0;
        }
        let diff = rtt_diff.unsigned_abs() as i32;
        let lower = self.tun.per_lower_bound;
        let mut logged = 0u32;
        if self.gp_saw_rec && !self.gp_no_rec_chg {
            let val = if timely_says == 2 {
                let high = self.decrease_highrtt(self.per_gp_rec, rtt);
                let alt = self.get_decrease(self.per_gp_rec, diff);
                high.min(alt)
            } else {
                self.get_decrease(self.per_gp_rec, diff)
            };
            if self.per_gp_rec > val {
                self.per_gp_rec = val;
            } else {
                self.per_gp_rec = lower;
            }
            self.per_gp_rec = self.per_gp_rec.max(lower);
            logged |= 1;
        }
        if self.gp_saw_ss {
            let val = if timely_says == 2 {
                let high = self.decrease_highrtt(self.per_gp_ss, rtt);
                let alt = self.get_decrease(self.per_gp_ss, diff);
                high.min(alt)
            } else {
                self.get_decrease(self.per_gp_ss, diff)
            };
            self.per_gp_ss = val.min(self.per_gp_ss).max(lower);
            logged |= 4;
        } else if self.gp_saw_ca {
            let val = if timely_says == 2 {
                let high = self.decrease_highrtt(self.per_gp_ca, rtt);
                let alt = self.get_decrease(self.per_gp_ca, diff);
                high.min(alt)
            } else {
                self.get_decrease(self.per_gp_ca, diff)
            };
            self.per_gp_ca = val.min(self.per_gp_ca).max(lower);
            logged |= 2;
        }
        if self.gp_timely_dec_cnt < 0x7 {
            self.gp_timely_dec_cnt += 1;
            if self.tun.timely_dec_clear != 0 && self.gp_timely_dec_cnt == self.tun.timely_dec_clear
            {
                self.gp_timely_dec_cnt = 0;
            }
        }
        self.log_timely(0, 2, [u64::from(logged), u64::from(lower), 0, 0]);
    }

    /// FreeBSD `rack_make_timely_judgement`.
    pub fn make_timely_judgement(&mut self, rtt: u32, rtt_diff: i32, prev_rtt: u32) -> i32 {
        let minrtt = self.gp_min_rtt;
        if u64::from(rtt) >= u64::from(minrtt) * u64::from(self.tun.rtt_maxmul) {
            self.log_timely(
                0,
                4,
                [u64::from(minrtt), u64::from(rtt), u64::from(prev_rtt), 2],
            );
            2
        } else if u64::from(rtt)
            <= u64::from(minrtt)
                + (u64::from(minrtt) * u64::from(self.tun.rtt_minmul)
                    / u64::from(self.tun.rtt_mindiv.max(1)))
        {
            self.log_timely(
                0,
                5,
                [u64::from(minrtt), u64::from(rtt), u64::from(prev_rtt), 0],
            );
            0
        } else if rtt_diff <= 0 {
            self.log_timely(
                0,
                6,
                [u64::from(minrtt), u64::from(rtt), u64::from(prev_rtt), 0],
            );
            0
        } else {
            self.log_timely(
                0,
                7,
                [u64::from(minrtt), u64::from(rtt), u64::from(prev_rtt), 1],
            );
            1
        }
    }

    /// FreeBSD `rack_in_gp_window`.
    #[must_use]
    pub fn in_gp_window(&self, stack: &RackStack) -> bool {
        stack.tp.t_flags & TF_GPUTINPROG != 0
    }

    /// FreeBSD `rack_mark_in_gp_win`.
    pub fn mark_in_gp_win(&mut self, stack: &mut RackStack) {
        stack.tp.t_flags |= TF_GPUTINPROG;
        self.app_limited_needs_set = false;
    }

    /// FreeBSD `rack_clear_gp_marks`.
    pub fn clear_gp_marks(&mut self, stack: &mut RackStack) {
        stack.tp.t_flags &= !TF_GPUTINPROG;
        self.app_limited_needs_set = false;
        self.gp_cumack_ts = 0;
    }

    /// FreeBSD `rack_tend_gp_marks`.
    pub fn tend_gp_marks(&mut self, stack: &RackStack) {
        self.log_gpset(stack, self.gput_ack, 0, 0, 0, 2, 0, 0);
    }

    /// FreeBSD `rack_log_gp_calc`.
    fn log_gp_calc(
        &mut self,
        addpart: u64,
        subpart: u64,
        srtt: u64,
        bytes_ps: u64,
        utim: u64,
        meth: u8,
    ) {
        self.event(
            9,
            0,
            [
                addpart,
                subpart,
                srtt,
                bytes_ps,
                utim,
                u64::from(meth),
                0,
                0,
            ],
        );
    }

    /// FreeBSD `rack_do_goodput_measurement`.
    #[allow(unused_assignments)] // The donor updates the same fields on several paths.
    pub fn do_goodput_measurement(&mut self, stack: &mut RackStack, sample: GpSample) -> Quality {
        let segsiz = Self::maxseg(stack).min(self.pace_min_segs.max(1));
        let tim = sample.now.saturating_sub(sample.gput_ts);
        let stim = sample.cumack_ts.saturating_sub(sample.output_ts);
        let utim = tim.max(stim).max(1);
        let reqbytes = Self::init_window(stack).min(MIN_GP_WIN.saturating_mul(segsiz));
        self.log_gpset(
            stack,
            sample.ack,
            sample.now as u32,
            sample.cumack_ts as u32,
            0,
            3,
            0,
            0,
        );
        let mut quality = Quality::None;
        let mut bytes_ps = 0u64;
        let mut bytes = 0u32;
        if (tim == 0 && stim == 0) || sample.lowrtt == u32::MAX {
            quality = Quality::None;
        } else {
            self.last_max_bw = u64::from(sample.high_rwnd).saturating_mul(HPTS_USEC_IN_SEC)
                / u64::from(sample.lowrtt.max(1));
            if sample.ack < sample.gput_seq {
                quality = Quality::None;
            } else {
                bytes = sample.ack.wrapping_sub(sample.gput_seq);
                let mut req = reqbytes;
                if !sample.filled {
                    req = req.saturating_sub(2 * segsiz);
                    self.prev_gp_srtt = sample.srtt;
                }
                bytes_ps = u64::from(bytes) * HPTS_USEC_IN_SEC / utim;
                if u64::from(bytes) < u64::from(req) || self.app_limited_needs_set {
                    quality = Quality::None;
                } else {
                    self.app_limited_needs_set = false;
                    let new_diff = sample.srtt as i32 - self.prev_gp_srtt as i32;
                    if !sample.filled {
                        self.rtt_diff = new_diff;
                    } else if !sample.saw_probe_rtt {
                        self.rtt_diff -= self.rtt_diff / 8;
                        self.rtt_diff += new_diff / 8;
                    }
                    self.gp_srtt = sample.srtt;
                    self.gp_min_rtt = self.gp_min_rtt.min(sample.srtt.max(1));
                    let timely =
                        self.make_timely_judgement(sample.srtt, self.rtt_diff, self.prev_gp_srtt);
                    if bytes_ps > self.last_max_bw {
                        bytes_ps = self.last_max_bw;
                    }
                    if !self.gp_filled {
                        if bytes_ps != 0 {
                            self.gp_bw = bytes_ps;
                            self.gp_filled = true;
                            self.num_measurements = 1;
                            quality = Quality::High;
                        }
                    } else if self.num_measurements < RACK_REQ_AVG {
                        self.gp_bw = self.gp_bw.saturating_add(bytes_ps);
                        self.num_measurements += 1;
                        if self.num_measurements >= RACK_REQ_AVG {
                            self.gp_bw /= u64::from(self.num_measurements);
                        }
                        quality = Quality::High;
                    } else {
                        if self.num_measurements < 0xff {
                            self.num_measurements += 1;
                        }
                        let srtt = u64::from(if sample.srtt == 0 {
                            if self.gp_min_rtt != u32::MAX {
                                self.gp_min_rtt
                            } else {
                                1000
                            }
                        } else {
                            sample.srtt
                        });
                        let (subpart, addpart, meth) = if !self.gp_dyn_mul {
                            let mut sub = self.gp_bw.saturating_mul(utim) / (srtt * 8);
                            let mut add = bytes_ps.saturating_mul(utim) / (srtt * 8);
                            if sub < self.gp_bw / 2 {
                                (sub, add, 1)
                            } else {
                                sub = self.gp_bw / 2;
                                add = bytes_ps / 2;
                                (sub, add, 2)
                            }
                        } else if utim <= srtt {
                            (
                                self.gp_bw.saturating_mul(utim)
                                    / (srtt * u64::from(self.tun.wma_divisor.max(1))),
                                bytes_ps.saturating_mul(utim)
                                    / (srtt * u64::from(self.tun.wma_divisor.max(1))),
                                3,
                            )
                        } else {
                            (
                                self.gp_bw / u64::from(self.tun.wma_divisor.max(1)),
                                bytes_ps / u64::from(self.tun.wma_divisor.max(1)),
                                4,
                            )
                        };
                        self.log_gp_calc(addpart, subpart, srtt, bytes_ps, utim, meth);
                        self.gp_bw = self.gp_bw.saturating_sub(subpart).saturating_add(addpart);
                        quality = Quality::High;
                        if !self.measure_saw_probe_rtt || bytes_ps > self.gp_bw {
                            self.decrease_bw_mul(timely, sample.srtt, self.rtt_diff);
                        }
                    }
                    if !self.measure_saw_probe_rtt && self.gp_rtt_set {
                        self.increase_bw_mul(
                            stack,
                            timely,
                            bytes_ps,
                            u64::from(self.gp_srtt),
                            false,
                        );
                    }
                    self.prev_gp_srtt = sample.srtt;
                    self.loss_at_start = self.loss_count;
                }
            }
        }
        self.gp_rtt_set = false;
        self.gp_saw_rec = false;
        self.gp_saw_ca = false;
        self.gp_saw_ss = false;
        self.dragged_bottom = false;
        stack.tp.t_flags &= !TF_GPUTINPROG;
        self.gp_output_ts = sample.output_ts;
        self.gp_cumack_ts = sample.cumack_ts;
        quality
    }

    /// FreeBSD `rack_set_prtt_target`.
    fn set_prtt_target(&mut self, stack: &RackStack, segsiz: u32, rtt: u32, now_us: u64) {
        let bwdp = self.get_bw(stack, now_us).saturating_mul(u64::from(rtt)) / HPTS_USEC_IN_SEC;
        let round = |v: u64| -> u32 {
            let s = u64::from(segsiz.max(1));
            (v.div_ceil(s).saturating_mul(s)) as u32
        };
        self.target_probertt_flight =
            round(bwdp).max(segsiz.saturating_mul(self.tun.timely_min_segs));
    }

    /// FreeBSD `rack_enter_probertt`.
    pub fn enter_probertt(&mut self, stack: &mut RackStack, now_us: u32) {
        self.probertt_lower = now_us;
        if !self.gp_dyn_mul {
            return;
        }
        if stack.tp.snd_max == stack.tp.snd_una {
            return;
        }
        if stack.tp.t_flags & TF_GPUTINPROG != 0 && stack.tp.snd_una > self.gput_seq {
            let sample = self.sample_from(stack, stack.tp.snd_una, now_us.into());
            self.do_goodput_measurement(stack, sample);
        }
        self.per_of_gp_probertt = self.tun.per_of_gp_probertt;
        self.probertt_entered = now_us;
        let segsiz = Self::maxseg(stack).min(self.pace_min_segs.max(1));
        self.in_probe_rtt = true;
        self.measure_saw_probe_rtt = true;
        self.probertt_starts = 0;
        self.entry_gp_rtt = self.gp_srtt;
        let rtt = if self.tun.probertt_use_min_rtt_entry {
            self.gp_min_rtt
        } else {
            self.gp_srtt
        };
        self.set_prtt_target(stack, segsiz, rtt, now_us.into());
        self.event(10, 0, [u64::from(now_us), u64::from(rtt), 0, 0, 0, 0, 0, 0]);
    }

    /// FreeBSD `rack_exit_probertt`.
    pub fn exit_probertt(&mut self, stack: &mut RackStack, now_us: u32) {
        let segsiz = Self::maxseg(stack).min(self.pace_min_segs.max(1));
        self.in_probe_rtt = false;
        if stack.tp.t_flags & TF_GPUTINPROG != 0 {
            if stack.tp.snd_una > self.gput_seq {
                let sample = self.sample_from(stack, stack.tp.snd_una, now_us.into());
                self.do_goodput_measurement(stack, sample);
            } else {
                stack.tp.t_flags &= !TF_GPUTINPROG;
            }
        }
        self.gp_incr = false;
        self.gp_incr_seen = false;
        if self.highly_buffered {
            self.per_gp_ca = self.tun.atexit_prtt_hbp;
            self.per_gp_ss = self.tun.atexit_prtt_hbp;
        } else {
            self.per_gp_ca = self.tun.atexit_prtt;
            self.per_gp_ss = self.tun.atexit_prtt;
        }
        self.rtt_diff = 0;
        if self.tun.probe_rtt_sets_cwnd != 0 {
            let setval = match self.tun.probertt_use_min_rtt_exit {
                1 => self.gp_min_rtt,
                2 => self.gp_srtt,
                3 => self.entry_gp_rtt,
                _ => {
                    let sum = u64::from(self.entry_gp_rtt) * 10 / u64::from(self.gp_srtt.max(1));
                    if sum >= 20 {
                        self.entry_gp_rtt
                    } else if sum >= 15 {
                        self.gp_srtt.min(self.entry_gp_rtt)
                    } else {
                        self.gp_min_rtt
                    }
                }
            };
            self.set_prtt_target(stack, segsiz, setval, now_us.into());
            let mut setto = self.target_probertt_flight;
            if self.tun.probe_rtt_sets_cwnd > 1 {
                setto += self.target_probertt_flight * self.tun.probe_rtt_sets_cwnd / 100;
            }
            stack.tp.snd_cwnd = setto
                .div_ceil(segsiz.max(1))
                .saturating_mul(segsiz.max(1))
                .max(segsiz.saturating_mul(self.tun.timely_min_segs));
            stack.tp.snd_ssthresh = stack.tp.snd_cwnd.saturating_sub(1);
        }
        self.probertt_entered = now_us;
        self.probertt_starts = now_us;
        self.probertt_lower = now_us;
        self.probertt_last = now_us;
        self.event(
            11,
            0,
            [
                u64::from(now_us),
                u64::from(self.per_gp_ca),
                0,
                0,
                0,
                0,
                0,
                0,
            ],
        );
    }

    fn sample_from(&mut self, stack: &mut RackStack, ack: u32, now_us: u64) -> GpSample {
        GpSample {
            ack,
            gput_seq: self.gput_seq,
            gput_ack: self.gput_ack,
            gput_ts: self.gput_ts,
            cumack_ts: self.gp_cumack_ts,
            output_ts: self.gp_output_ts,
            lowrtt: self.gp_lowrtt,
            high_rwnd: self.gp_high_rwnd,
            srtt: self.gp_srtt,
            prev_srtt: self.prev_gp_srtt,
            now: now_us,
            snd_wnd: stack.snd_wnd,
            filled: self.gp_filled,
            saw_probe_rtt: self.measure_saw_probe_rtt,
        }
    }

    /// FreeBSD `rack_check_probe_rtt`.
    pub fn check_probe_rtt(&mut self, stack: &mut RackStack, now_us: u32, flight: u32) {
        if !self.gp_filled {
            return;
        }
        if self.in_probe_rtt {
            let srtt = self.gp_srtt.max(1);
            if self.went_idle_time != 0
                && now_us.saturating_sub(self.went_idle_time) > self.tun.min_probertt_hold
            {
                self.exit_probertt(stack, now_us);
                return;
            }
            if self.tun.probe_rtt_safety != 0
                && now_us > self.probertt_entered
                && now_us - self.probertt_entered > self.tun.probe_rtt_safety
            {
                self.event(12, 0, [u64::from(now_us), 0, 0, 0, 0, 0, 0, 0]);
                self.exit_probertt(stack, now_us);
                return;
            }
            let mut endtime = self
                .probertt_entered
                .saturating_add(srtt.saturating_mul(self.tun.max_drain_wait));
            if self.highly_buffered {
                endtime = endtime.saturating_add(srtt.saturating_mul(self.tun.max_drain_hbp));
            }
            let must_stay = self
                .probertt_entered
                .saturating_add(srtt.saturating_mul(self.tun.must_drain));
            if flight > self.target_probertt_flight && now_us < endtime {
                if now_us > self.probertt_entered {
                    let calc = (now_us - self.probertt_entered) / srtt;
                    if calc != 0 {
                        let reduce = calc.saturating_mul(self.tun.per_of_gp_probertt_reduce);
                        if reduce > self.per_of_gp_probertt {
                            self.per_of_gp_probertt = self.tun.per_of_gp_lowthresh;
                        } else {
                            self.per_of_gp_probertt =
                                self.per_of_gp_probertt.saturating_sub(reduce);
                        }
                        self.per_of_gp_probertt =
                            self.per_of_gp_probertt.max(self.tun.per_of_gp_lowthresh);
                    }
                }
                return;
            }
            if self.probertt_starts == 0 {
                if (now_us < must_stay && self.highly_buffered)
                    || flight > self.target_probertt_flight
                {
                    if now_us > self.probertt_entered {
                        let calc = (now_us - self.probertt_entered) / srtt;
                        if calc != 0 {
                            let reduce = calc.saturating_mul(self.tun.per_of_gp_probertt_reduce);
                            self.per_of_gp_probertt = self
                                .per_of_gp_probertt
                                .saturating_sub(reduce)
                                .max(self.tun.per_of_gp_lowthresh);
                        }
                    }
                    return;
                }
                self.probertt_starts = now_us.max(1);
                self.per_of_gp_probertt = self.tun.per_of_gp_probertt;
            }
            let end = self
                .probertt_starts
                .saturating_add(self.tun.min_probertt_hold);
            if now_us >= end {
                self.exit_probertt(stack, now_us);
            }
        } else if !self.skip_timely
            && now_us > self.probertt_lower
            && now_us - self.probertt_lower >= self.tun.time_between_probertt
        {
            self.enter_probertt(stack, now_us);
        }
    }

    /// FreeBSD `rack_in_gp_window` state helper for callers without a stack.
    #[must_use]
    pub fn gp_window_open(&self) -> bool {
        self.gp_saw_ca || self.gp_saw_ss || self.gp_saw_rec
    }

    /// FreeBSD `rack_log_hybrid`.
    fn log_hybrid(
        &mut self,
        stack: &RackStack,
        seq: u32,
        track: Option<SendfileTrack>,
        mode: u8,
        line: u32,
        err: i32,
    ) {
        let ent = track.unwrap_or_default();
        self.event(
            13,
            line,
            [
                u64::from(seq),
                u64::from(ent.start),
                u64::from(ent.end),
                u64::from(ent.hybrid_flags),
                u64::from(mode),
                err as u64,
                u64::from(stack.tp.snd_una),
                0,
            ],
        );
    }

    /// FreeBSD `rack_set_dgp_hybrid_mode`.
    pub fn set_dgp_hybrid_mode(
        &mut self,
        stack: &mut RackStack,
        seq: u32,
        len: u32,
        cts: u64,
        track: Option<SendfileTrack>,
    ) {
        let orig = self.last_sft;
        let Some(mut cur) = track else {
            if self.hybrid_mode {
                self.client_suggested_maxseg = 0;
                self.catch_up = false;
                if !self.cspr_is_fcc {
                    self.bw_rate_cap = 0;
                } else {
                    self.fillcw_cap = self.tun.fillcw_bw_cap;
                }
            }
            self.log_hybrid(
                stack,
                seq.wrapping_add(len.saturating_sub(1)),
                None,
                4,
                0,
                0,
            );
            self.last_sft = None;
            return;
        };
        if cur.hybrid_flags & HYBRID_WASSET == 0 {
            if self.hybrid_mode {
                self.client_suggested_maxseg = 0;
                self.catch_up = false;
                self.bw_rate_cap = 0;
            }
            self.last_sft = None;
            if cur.flags & TRK_FSND == 0 {
                cur.flags |= TRK_FSND;
                cur.first_send = cts;
            }
            self.last_sft = Some(cur);
            return;
        }
        if self.last_sft.map(|t| t.start_seq) == Some(cur.start_seq) {
            self.log_hybrid(stack, seq, Some(cur), 10, 0, 0);
            return;
        }
        if !self.hybrid_mode {
            self.last_sft = Some(cur);
            self.log_hybrid(stack, seq, Some(cur), 5, 0, 0);
            return;
        }
        if cur.hybrid_flags & HYBRID_CSPR != 0 && cur.cspr != 0 {
            if !self.cspr_is_fcc {
                self.bw_rate_cap = self.compensate_for_linerate(stack, cur.cspr);
            } else {
                self.fillcw_cap = self.compensate_for_linerate(stack, cur.cspr);
            }
        } else if !self.cspr_is_fcc {
            self.bw_rate_cap = 0;
        } else {
            self.fillcw_cap = self.tun.fillcw_bw_cap;
        }
        self.client_suggested_maxseg = if cur.hybrid_flags & HYBRID_H_MS != 0 {
            cur.hint_maxseg
        } else {
            0
        };
        if cur.timestamp == self.last_tm_mark {
            cur.hybrid_flags |= HYBRID_SENDTIME;
        }
        if cur.hybrid_flags & HYBRID_CU != 0 && cur.cspr > 0 {
            self.catch_up = true;
            cur.deadline = if cur.hybrid_flags & HYBRID_SENDTIME != 0 {
                cts
            } else {
                cur.localtime
            };
            let length = u64::from(cur.end.saturating_sub(cur.start));
            cur.deadline = cur
                .deadline
                .saturating_add(length.saturating_mul(HPTS_USEC_IN_SEC) / cur.cspr.max(1));
        } else {
            self.catch_up = false;
            cur.deadline = 0;
        }
        if self.client_suggested_maxseg != 0 {
            self.set_pace_segments(stack, 0, None);
        }
        let _ = orig;
        self.last_sft = Some(cur);
        self.last_tm_mark = cur.timestamp;
        self.log_hybrid(stack, seq, Some(cur), 5, 0, 0);
    }

    /// FreeBSD `rack_chk_req_and_hybrid_on_out`.
    pub fn chk_req_and_hybrid_on_out(
        &mut self,
        stack: &mut RackStack,
        seq: u32,
        len: u32,
        cts: u64,
        track: Option<SendfileTrack>,
    ) {
        let need = self
            .last_sft
            .map(|t| t.flags == TRK_EMPTY || seq >= t.end_seq)
            .unwrap_or(true);
        if need {
            self.set_dgp_hybrid_mode(stack, seq, len, cts, track);
        }
        let Some(mut ent) = self.last_sft else {
            return;
        };
        if ent.end_seq < seq.wrapping_add(len) {
            ent.end_seq = seq.wrapping_add(len);
            if self.hybrid_mode {
                self.log_hybrid_bw(stack, seq, u64::from(len), 0, 0, 14, 0, Some(ent));
            }
        }
        if ent.flags & TRK_FSND == 0 {
            ent.flags |= TRK_FSND;
            ent.first_send = cts;
            ent.sent_at_fs = stack.bytes_transmitted;
            ent.rxt_at_fs = stack.bytes_retransmitted;
        }
        self.last_sft = Some(ent);
    }

    /// FreeBSD `rack_set_dgp`.
    fn set_dgp(&mut self, stack: &mut RackStack) -> Result<(), &'static str> {
        if self.dgp_on {
            return Ok(());
        }
        if self.use_fixed_rate && self.always_pace {
            return Err("EBUSY");
        }
        if self.always_pace {
            stack.os.pacing.rack_remove_pacing();
        }
        self.pacing_method |= RACK_DGP_PACING;
        self.dgp_on = true;
        self.always_pace = true;
        self.use_fixed_rate = false;
        if self.gp_ready {
            stack.os.pacing.rack_set_cc_pacing();
        }
        stack.t_flags2 |= TF2_SUPPORTS_MBUFQ;
        stack.options.full_size_rxt = true;
        stack.options.shape_rxt_to_pacing_min = false;
        stack.options.use_cmp_ack = true;
        if stack.tp.t_state != 0 {
            stack.t_flags2 |= TF2_MBUF_ACKCMP;
        }
        stack.options.enable_scwnd = true;
        self.gp_dyn_mul = true;
        self.per_gp_ca = 100;
        stack.options.rr_config = 3;
        self.pace_to_cwnd = true;
        self.per_gp_rec = 90;
        Ok(())
    }

    /// FreeBSD `rack_clear_rate_sample`.
    pub fn clear_rate_sample(&mut self) {
        self.gp_srtt = 0;
        self.gp_rtt_set = false;
    }

    /// FreeBSD `rack_set_pace_segments`.
    fn set_pace_segments(
        &mut self,
        stack: &mut RackStack,
        line: u32,
        fill_override: Option<u64>,
    ) -> bool {
        if self.hybrid_mode
            && self.pace_max_segs != 0
            && let Some(mut ent) = self.last_sft
        {
            ent.hybrid_flags &= !HYBRID_SETMSS;
            self.last_sft = Some(ent);
            return false;
        }
        let orig_min = self.pace_min_segs;
        let orig_max = self.pace_max_segs;
        let maxseg = Self::maxseg(stack);
        let user_max = maxseg.saturating_mul(if stack.options.force_max_seg {
            stack.options.min_segs as u32 + 1
        } else {
            0
        });
        let mut changed = false;
        if maxseg != self.pace_min_segs {
            changed = true;
        }
        self.pace_min_segs = maxseg;
        if (self.use_fixed_rate || stack.options.force_max_seg) && user_max != self.pace_max_segs {
            changed = true;
        }
        if stack.options.force_max_seg || self.use_fixed_rate {
            self.pace_max_segs = user_max.max(maxseg);
        } else if self.always_pace {
            if self.gp_bw != 0 || self.init_rate != 0 {
                let rate = fill_override.unwrap_or_else(|| self.get_gp_est(stack, 0));
                let orig = self.pace_max_segs;
                self.pace_max_segs = if rate != 0 {
                    Self::pace_len(stack, rate, maxseg)
                } else {
                    self.pace_min_segs
                };
                if orig != self.pace_max_segs {
                    changed = true;
                }
            } else if self.pace_max_segs == 0 {
                changed = true;
                self.pace_max_segs = Self::init_window(stack);
            }
        }
        if self.pace_max_segs > PACE_MAX_IP_BYTES {
            changed = true;
            self.pace_max_segs = PACE_MAX_IP_BYTES;
        }
        if changed {
            self.log_type_pacing_sizes(stack, orig_min, orig_max, line, 2);
        }
        changed
    }

    /// FreeBSD `rack_arrive_at_discounted_rate`.
    pub fn arrive_at_discounted_rate(
        &mut self,
        stack: &RackStack,
        window_input: u64,
        mut rate_set: bool,
    ) -> (u64, bool, u32) {
        let (reduced, gain);
        if window_input < u64::from(Self::init_window(stack)) {
            rate_set = true;
            reduced = self.get_lt_bw(stack, 0);
            gain = 100;
        } else if Self::in_recovery(stack) {
            if !stack.options.rack_hibeta {
                reduced = window_input / 2;
                gain = 50;
            } else {
                reduced = window_input.saturating_mul(u64::from(stack.options.saved_beta)) / 100;
                gain = stack.options.saved_beta;
            }
        } else {
            gain = self.get_output_gain(stack, false);
            let gain = gain.min(self.tun.gain_p5_ub);
            reduced = window_input.saturating_mul(u64::from(gain)) / 100;
        }
        (reduced, rate_set, gain)
    }

    /// FreeBSD `pace_to_fill_cwnd`.
    #[allow(clippy::too_many_arguments)] // Donor signature.
    pub fn pace_to_fill_cwnd(
        &mut self,
        stack: &RackStack,
        pacing_delay: i64,
        len: u32,
        _segsiz: u32,
        rate_wanted: &mut u64,
        non_paced: bool,
    ) -> i64 {
        self.via_fill_cw = false;
        if u64::from(self.cwnd_to_use) < u64::from(stack.tp.snd_cwnd) && self.last_us_rtt == 0 {
            return pacing_delay;
        }
        if self.last_us_rtt == 0 {
            return pacing_delay;
        }
        if self.pace_fill_if_rttin_range
            && self.last_us_rtt >= self.gp_min_rtt.saturating_mul(self.rtt_limit_mul.max(1))
        {
            return pacing_delay;
        }
        if self.fillcw_cap != 0 && *rate_wanted >= self.fillcw_cap {
            return pacing_delay;
        }
        let mut fill_bw = u64::from(stack.tp.snd_cwnd.min(self.cwnd_to_use));
        let mut rate_set = false;
        if stack.options.pace_fill_if_rttin_range {
            let (reduced, set, _) = self.arrive_at_discounted_rate(stack, fill_bw, false);
            fill_bw = reduced;
            rate_set = set;
        }
        if fill_bw > u64::from(stack.snd_wnd) {
            fill_bw = u64::from(stack.snd_wnd);
        }
        if !rate_set {
            fill_bw = fill_bw.saturating_mul(HPTS_USEC_IN_SEC) / u64::from(self.last_us_rtt.max(1));
        }
        if self.fillcw_cap != 0 && fill_bw >= self.fillcw_cap {
            fill_bw = self.fillcw_cap;
        }
        if self.tun.bw_multipler > 0 {
            let gp = self.get_gp_est(stack, 0);
            let lt = self.get_lt_bw(stack, 0);
            let rate = gp.max(lt).saturating_mul(u64::from(self.tun.bw_multipler)) / 100;
            if fill_bw > rate {
                fill_bw = rate;
            }
        }
        if non_paced {
            *rate_wanted = fill_bw;
        }
        if fill_bw < RACK_MIN_BW || fill_bw < *rate_wanted {
            return pacing_delay;
        }
        self.via_fill_cw = true;
        let lentim = u64::from(len) * HPTS_USEC_IN_SEC;
        (lentim / fill_bw.max(1)) as i64
    }

    /// FreeBSD `rack_log_pacing_delay_calc`.
    #[allow(clippy::too_many_arguments)] // Donor signature.
    fn log_pacing_delay_calc(
        &mut self,
        len: u64,
        delay: i64,
        rate: u64,
        flex: u64,
        stim: u64,
        line: u32,
        mode: u8,
    ) {
        self.event(
            14,
            line,
            [len, delay as u64, rate, flex, stim, u64::from(mode), 0, 0],
        );
    }

    /// FreeBSD `rack_get_pacing_delay`.
    pub fn get_pacing_delay(
        &mut self,
        stack: &mut RackStack,
        len: u32,
        retransmit: bool,
        segsiz: u32,
        now_us: u64,
    ) -> i64 {
        let mut delay;
        if !self.always_pace {
            let srtt = if stack.tp.t_srtt != 0 {
                u64::from(stack.tp.t_srtt)
            } else {
                1
            };
            let cwnd = if stack.tp.snd_cwnd != 0 {
                u64::from(stack.tp.snd_cwnd)
            } else {
                u64::from(self.cwnd_to_use)
            };
            let mut tr_perms = cwnd.saturating_mul(1000) / srtt;
            if tr_perms == 0 {
                tr_perms = u64::from(Self::maxseg(stack));
            }
            delay = i64::from(len) / tr_perms.max(1) as i64;
            if delay != 0 && stack.os.pacing.delay_reduction != 0 {
                let reduce = delay / i64::from(stack.os.pacing.delay_reduction);
                delay = delay.saturating_sub(reduce).max(0);
            }
            delay = delay.saturating_mul(HPTS_USEC_IN_MSEC as i64);
            if self.pace_to_cwnd {
                let mut rate = 0;
                delay = self.pace_to_fill_cwnd(stack, delay, len, segsiz, &mut rate, true);
                self.ack_can_sendout_data = true;
                self.log_pacing_delay_calc(u64::from(len), delay, rate, 0, 0, 0, 14);
            } else {
                self.log_pacing_delay_calc(u64::from(len), delay, tr_perms, 0, 0, 0, 7);
            }
            return delay;
        }
        if stack.options.rr_config == 1 && retransmit {
            return i64::from(self.min_to);
        }
        let (bw_est, mut rate_wanted);
        if self.use_fixed_rate {
            bw_est = self.get_fixed_pacing_bw(stack);
            rate_wanted = bw_est;
        } else if self.init_rate == 0 && self.gp_bw == 0 {
            bw_est = 0;
            rate_wanted = 0;
        } else if self.dgp_on {
            bw_est = self.get_bw(stack, now_us);
            let (out, _) = self.get_output_bw(stack, bw_est, retransmit);
            rate_wanted = out;
        } else {
            rate_wanted = u64::from(stack.tp.snd_cwnd.min(self.cwnd_to_use));
            let (win, rate_set, gain) = self.arrive_at_discounted_rate(stack, rate_wanted, false);
            rate_wanted = win;
            if !rate_set {
                rate_wanted = rate_wanted.min(u64::from(stack.snd_wnd));
                rate_wanted = rate_wanted.saturating_mul(HPTS_USEC_IN_SEC)
                    / u64::from(self.last_us_rtt.max(1));
            }
            bw_est = rate_wanted;
            self.log_pacing_delay_calc(
                u64::from(stack.tp.snd_cwnd),
                self.cwnd_to_use as i64,
                rate_wanted,
                bw_est,
                u64::from(self.last_us_rtt),
                0,
                88,
            );
            let _ = gain;
        }
        if (bw_est == 0 || rate_wanted == 0 || !self.gp_ready) && !self.use_fixed_rate {
            return self.get_pacing_delay_old(stack, len, segsiz);
        }
        let (capped_bw, _) = self.rate_cap_bw(stack, rate_wanted, now_us);
        rate_wanted = capped_bw;
        let segs = (len + segsiz.saturating_sub(1)) / segsiz.max(1);
        let mut oh = Self::maxseg(stack).saturating_sub(segsiz) + 20;
        oh += if self.is_v6 { 40 } else { 20 };
        oh += 14;
        let lentim = u64::from(len + segs.saturating_mul(oh)).saturating_mul(HPTS_USEC_IN_SEC);
        delay = (lentim / rate_wanted.max(1)) as i64;
        if self.tun.limit_time_with_srtt && !self.use_fixed_rate {
            let srtt = if stack.tp.t_srtt != 0 {
                u64::from(stack.tp.t_srtt)
            } else {
                u64::from(RACK_INITIAL_RTO)
            };
            if srtt < delay as u64 {
                self.log_pacing_delay_calc(srtt, delay, rate_wanted, bw_est, lentim, 0, 99);
                delay = srtt as i64;
            }
        }
        self.log_pacing_delay_calc(u64::from(len), delay, rate_wanted, bw_est, 0, 0, 2);
        delay
    }

    fn get_pacing_delay_old(&mut self, stack: &RackStack, len: u32, _segsiz: u32) -> i64 {
        let srtt = if stack.tp.t_srtt != 0 {
            u64::from(stack.tp.t_srtt)
        } else {
            1
        };
        let cwnd = if stack.tp.snd_cwnd != 0 {
            u64::from(stack.tp.snd_cwnd)
        } else {
            u64::from(self.cwnd_to_use)
        };
        let mut tr_perms = cwnd.saturating_mul(1000) / srtt;
        if tr_perms == 0 {
            tr_perms = u64::from(Self::maxseg(stack).max(_segsiz));
        }
        let mut delay = i64::from(len) / tr_perms.max(1) as i64;
        if delay != 0 && stack.os.pacing.delay_reduction != 0 {
            let reduce = delay / i64::from(stack.os.pacing.delay_reduction);
            delay = delay.saturating_sub(reduce).max(0);
        }
        delay.saturating_mul(HPTS_USEC_IN_MSEC as i64)
    }

    /// FreeBSD `rack_start_gp_measurement`.
    pub fn start_gp_measurement(
        &mut self,
        stack: &mut RackStack,
        startseq: u32,
        sb_offset: u32,
        now_us: u64,
        sb_avail: u64,
        last_cumack_advance: u64,
    ) {
        if stack.tp.t_state == 0 {
            return;
        }
        if stack.tp.t_state >= 4
            && sb_avail
                < u64::from(
                    Self::init_window(stack).max(MIN_GP_WIN.saturating_mul(Self::maxseg(stack))),
                )
        {
            return;
        }
        stack.tp.t_flags |= TF_GPUTINPROG;
        self.gp_cumack_ts = 0;
        self.gp_lowrtt = u32::MAX;
        self.gp_high_rwnd = stack.snd_wnd;
        self.gput_seq = startseq;
        self.app_limited_needs_set = false;
        if self.in_probe_rtt {
            self.measure_saw_probe_rtt = true;
        } else if self.measure_saw_probe_rtt && self.gput_seq >= self.gput_ack {
            self.measure_saw_probe_rtt = false;
        }
        if self.gp_filled {
            self.gput_ts = last_cumack_advance;
        } else {
            self.gput_ts = now_us;
            self.gp_output_ts = now_us;
        }
        if !self.gp_filled && self.init_rate == 0 {
            self.app_limited_needs_set = true;
            self.gput_ack = startseq.saturating_add(
                Self::init_window(stack).max(MIN_GP_WIN.saturating_mul(Self::maxseg(stack))),
            );
            self.log_pacing_delay_calc(
                u64::from(self.gput_seq),
                self.gput_ack as i64,
                0,
                self.gput_ts,
                0,
                0,
                9,
            );
            self.tend_gp_marks(stack);
            self.log_gpset(stack, self.gput_ack, 0, 0, 0, 1, 0, 0);
            return;
        }
        if sb_offset != 0 && !self.gp_filled {
            self.app_limited_needs_set = true;
        }
        self.gput_ack = startseq.saturating_add(self.get_measure_window(stack, now_us));
        self.gp_cumack_ts = 0;
        self.log_pacing_delay_calc(
            u64::from(self.gput_seq),
            self.gput_ack as i64,
            0,
            self.gput_ts,
            0,
            0,
            9,
        );
        self.tend_gp_marks(stack);
        self.log_gpset(stack, self.gput_ack, 0, 0, 0, 1, 0, 0);
    }

    /// FreeBSD `rack_get_hpts_pacing_min_for_bw`.
    #[must_use]
    pub fn get_hpts_pacing_min_for_bw(&self, segsiz: u32, min_sleep_us: u64) -> u32 {
        let maxlen = self.gp_bw.saturating_mul(min_sleep_us) / HPTS_USEC_IN_SEC;
        let s = u64::from(segsiz.max(1));
        (maxlen.div_ceil(s).saturating_mul(s)) as u32
    }

    /// FreeBSD `rack_gain_for_fastoutput`: extend the fast-path block by the acked bytes.
    pub fn gain_for_fastoutput(
        &self,
        stack: &RackStack,
        left_to_send: u32,
        acked: u32,
        sb_avail: u32,
    ) -> u32 {
        let outstanding = stack.tp.snd_max.saturating_sub(stack.tp.snd_una);
        let gating = sb_avail
            .saturating_sub(outstanding)
            .min(stack.snd_wnd.saturating_sub(outstanding));
        let new_total = left_to_send.saturating_add(acked);
        if new_total <= gating {
            new_total
        } else {
            left_to_send
        }
    }

    /// FreeBSD `rack_set_dgp_hybrid_mode` helper for the request-derived mode.
    pub fn process_hybrid_pacing(
        &mut self,
        stack: &mut RackStack,
        request: HybridRequest,
        now_us: u64,
    ) -> Result<(), &'static str> {
        let mut track = SendfileTrack {
            flags: TRK_USED,
            start: request.start,
            end: request.end,
            start_seq: request.start,
            end_seq: request.end,
            localtime: now_us,
            ..SendfileTrack::default()
        };
        if request.hybrid_flags & HYBRID_ENABLE == 0 {
            if self.hybrid_mode {
                self.hybrid_mode = false;
                self.client_suggested_maxseg = 0;
            }
            self.log_hybrid(stack, request.start, Some(track), 1, 0, 0);
            return Ok(());
        }
        if !self.dgp_on {
            self.set_dgp(stack)?;
        }
        if !self.hybrid_mode {
            stack.os.pacing.method = crate::rack_os::PacingMethod::Reg;
            self.pacing_method |= RACK_REG_PACING;
            self.hybrid_mode = true;
            self.pacing_method &= !RACK_DGP_PACING;
        }
        track.hybrid_flags = request.hybrid_flags | HYBRID_WASSET;
        track.cspr = if request.hybrid_flags & HYBRID_CSPR != 0 {
            request.cspr
        } else {
            0
        };
        track.hint_maxseg = if request.hybrid_flags & HYBRID_H_MS != 0 {
            request.hint_maxseg
        } else {
            0
        };
        if request.hybrid_flags & HYBRID_CSPR != 0 {
            if !self.cspr_is_fcc {
                self.bw_rate_cap = self.compensate_for_linerate(stack, track.cspr);
            } else {
                self.fillcw_cap = self.compensate_for_linerate(stack, track.cspr);
            }
        }
        if request.hybrid_flags & HYBRID_CU != 0 && track.cspr > 0 {
            self.catch_up = true;
            track.deadline = now_us.saturating_add(
                u64::from(track.end.saturating_sub(track.start)).saturating_mul(HPTS_USEC_IN_SEC)
                    / track.cspr,
            );
        }
        self.last_sft = Some(track);
        self.last_tm_mark = track.timestamp;
        self.log_hybrid(stack, request.start, Some(track), 3, 0, 0);
        Ok(())
    }

    /// A borrowed view of the K314 pacing context (the donor's `r_ctl.pacing_method`).
    #[must_use]
    pub fn pacing_method(&self, stack: &RackStack) -> u32 {
        let mut method = self.pacing_method;
        match stack.os.pacing.method {
            crate::rack_os::PacingMethod::Reg => method |= RACK_REG_PACING,
            crate::rack_os::PacingMethod::Dgp => method |= RACK_DGP_PACING,
            crate::rack_os::PacingMethod::None => {}
        }
        method
    }

    /// The K314 `RackOs` view used by the functions above.
    #[must_use]
    pub fn os<'a>(&self, stack: &'a RackStack) -> &'a RackOs {
        &stack.os
    }
}

#[cfg(test)]
#[path = "rack_bbr_ut.rs"]
mod tests;
