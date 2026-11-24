// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD at donor revision 133344405f532bd54ff3629a77d4c4e062f01fdb:
//   sys/netinet/tcp_var.h:599-627  struct tcp_function_block and every tfb_* callback
//   sys/netinet/tcp_stacks/rack.c:24075-24098  struct tcp_function_block __tcp_rack
//   sys/netinet/tcp_stacks/rack.c  the rack_* callback bodies mapped below
// BSD-2/3. The kernel tcpcb/mbuf pointers, locks and callouts are mapped to the K314 RackOs and
// RackTcpcb software types; the callbacks perform their state transitions here, not counters.

//! FreeBSD `tcp_function_block` transport vtable for RACK.
//!
//! `RackVtable` is the donor-shaped callback set; `RackStack` implements every callback installed
//! by `struct tcp_function_block __tcp_rack` over the K314 [`RackOs`] context. Output, ACK
//! accounting, queued input, UMA init/fini, HPTS stop/rearm, handoff, MTU, socket-option,
//! hardware-TLS, query, switch-failed, early-wake, pipe, stack-info and inherit paths all change
//! real state and are tested.

use alloc::collections::VecDeque;

use crate::seq::{seq_ge, seq_gt, seq_lt};
use crate::{Errno, PacingMethod, RackOs, RackTcpcb};

/// FreeBSD `TCP_FUNC_OUTPUT_CANDROP`.
pub const TCP_FUNC_OUTPUT_CANDROP: u32 = 0x0001;
/// FreeBSD `TCP_FUNC_DEFAULT_OK`.
pub const TCP_FUNC_DEFAULT_OK: u32 = 0x0002;
/// FreeBSD `TCP_FUNCTION_NAME_LEN_MAX`.
pub const TCP_FUNCTION_NAME_LEN_MAX: usize = 32;
/// FreeBSD `TCPS_CLOSED`.
pub const TCPS_CLOSED: u8 = 0;
/// FreeBSD `TCPS_LISTEN`.
pub const TCPS_LISTEN: u8 = 1;
/// FreeBSD `TCPS_SYN_SENT`.
pub const TCPS_SYN_SENT: u8 = 2;
/// FreeBSD `TCPS_SYN_RECEIVED`.
pub const TCPS_SYN_RECEIVED: u8 = 3;
/// FreeBSD `TCPS_ESTABLISHED`.
pub const TCPS_ESTABLISHED: u8 = 4;
/// FreeBSD `TCPS_CLOSE_WAIT`.
pub const TCPS_CLOSE_WAIT: u8 = 5;
/// FreeBSD `TH_FIN`.
pub const TH_FIN: u16 = 0x01;
/// FreeBSD `TH_RST`.
pub const TH_RST: u16 = 0x04;
/// FreeBSD `TH_ACK`.
pub const TH_ACK: u16 = 0x10;
/// FreeBSD `TF_SENTFIN`.
pub const TF_SENTFIN: u32 = 0x0000_0010;
/// FreeBSD `TF_SACK_PERMIT`.
pub const TF_SACK_PERMIT: u32 = 0x0000_0200;
/// FreeBSD `TF2_MBUF_ACKCMP`.
pub const TF2_MBUF_ACKCMP: u32 = 0x0000_8000;
/// FreeBSD `TF2_SUPPORTS_MBUFQ`.
pub const TF2_SUPPORTS_MBUFQ: u32 = 0x0001_0000;
/// FreeBSD `PRUS_OOB`.
pub const PRUS_OOB: i32 = 0x1;
/// FreeBSD `PACE_TMR_DELACK`.
pub const PACE_TMR_DELACK: u32 = 0x01;
/// FreeBSD `PACE_TMR_RACK`.
pub const PACE_TMR_RACK: u32 = 0x02;
/// FreeBSD `PACE_TMR_TLP`.
pub const PACE_TMR_TLP: u32 = 0x04;
/// FreeBSD `PACE_TMR_RXT`.
pub const PACE_TMR_RXT: u32 = 0x08;
/// FreeBSD `PACE_TMR_PERSIT`.
pub const PACE_TMR_PERSIT: u32 = 0x10;
/// FreeBSD `PACE_TMR_KEEP`.
pub const PACE_TMR_KEEP: u32 = 0x20;
/// FreeBSD `PACE_PKT_OUTPUT`.
pub const PACE_PKT_OUTPUT: u32 = 0x40;
/// FreeBSD `PACE_TMR_MASK`.
pub const PACE_TMR_MASK: u32 =
    PACE_TMR_KEEP | PACE_TMR_PERSIT | PACE_TMR_RXT | PACE_TMR_TLP | PACE_TMR_RACK | PACE_TMR_DELACK;
/// FreeBSD `HPTS_USECS_PER_SLOT`.
pub const HPTS_USECS_PER_SLOT: u32 = 10;
/// FreeBSD `TCP_QUERY_SENDMAP`.
pub const TCP_QUERY_SENDMAP: i32 = 1;
/// FreeBSD `TCP_QUERY_TIMERS_UP`.
pub const TCP_QUERY_TIMERS_UP: i32 = 2;
/// FreeBSD `TCP_QUERY_RACK_TIMES`.
pub const TCP_QUERY_RACK_TIMES: i32 = 3;
/// FreeBSD `IPPROTO_TCP`.
pub const IPPROTO_TCP: i32 = 6;
/// FreeBSD `TCP_INFO`.
pub const TCP_INFO: i32 = 32;
/// FreeBSD `TCP_RACK_PACE_MAX_SEG`.
pub const TCP_RACK_PACE_MAX_SEG: i32 = 1054;
/// FreeBSD `TCP_RACK_PACING_BETA`.
pub const TCP_RACK_PACING_BETA: i32 = 1138;
/// FreeBSD `TCP_RACK_PACING_DIVISOR`.
pub const TCP_RACK_PACING_DIVISOR: i32 = 1146;
/// FreeBSD `TCP_RACK_PACE_MIN_SEG`.
pub const TCP_RACK_PACE_MIN_SEG: i32 = 1147;
/// FreeBSD `TCP_PACING_DND`.
pub const TCP_PACING_DND: i32 = 1151;
/// FreeBSD `MAX_USER_SET_SEG`.
pub const MAX_USER_SET_SEG: u32 = 0x3f;
/// FreeBSD `RL_MIN_DIVISOR`.
pub const RL_MIN_DIVISOR: u32 = 50;
/// FreeBSD `rack_default_pacing_divisor`.
pub const RACK_DEFAULT_PACING_DIVISOR: u32 = 250;
/// FreeBSD `rack_rxt_scoreboard_clear_thresh`.
pub const RACK_RXT_SCOREBOARD_CLEAR_THRESH: u8 = 2;

/// The `tcp_function_block` metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RackFunctionBlock {
    /// `tfb_tcp_block_name`.
    pub name: &'static str,
    /// `tfb_refcnt`.
    pub refcnt: u32,
    /// `tfb_flags`.
    pub flags: u32,
    /// `tfb_id`.
    pub id: u8,
}

impl RackFunctionBlock {
    /// The `__tcp_rack` function block from `rack.c`.
    #[must_use]
    pub const fn rack() -> Self {
        Self {
            name: "rack",
            refcnt: 1,
            flags: TCP_FUNC_OUTPUT_CANDROP | TCP_FUNC_DEFAULT_OK,
            id: 1,
        }
    }
}

/// The RACK stack lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackPortState {
    /// `tfb_tcp_fb_init` has not run.
    Uninitialized,
    /// The stack owns its `tcp_rack` allocation.
    Running,
    /// `tfb_tcp_fb_fini` ran.
    Fini,
}

/// One input segment mapped from `struct mbuf`/`struct tcphdr`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RackSegment {
    /// `th_seq`.
    pub seq: u32,
    /// `th_ack`.
    pub ack: u32,
    /// TCP flags.
    pub flags: u16,
    /// Payload bytes (`tlen`).
    pub payload_len: u32,
    /// Advertised window.
    pub window: u16,
    /// IP ECN codepoint.
    pub ecn: u8,
}

/// The result of `rack_do_segment_nounlock`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackSegmentResult {
    /// The segment was not actionable.
    Ignored,
    /// The ACK advanced `snd_una` by this many bytes.
    Acked(u32),
    /// A duplicate ACK.
    DuplicateAck,
    /// A FIN was observed.
    Fin,
    /// A reset was observed (the donor returns 1: the tcb is gone).
    Reset,
    /// Data was observed without a usable ACK.
    Data,
}

/// The `sockopt` direction (`enum sopt_dir`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SockoptDir {
    /// `SOPT_GET`.
    Get,
    /// `SOPT_SET`.
    Set,
}

/// One socket option mapped from `struct sockopt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RackSockopt {
    /// `sopt_dir`.
    pub dir: SockoptDir,
    /// `sopt_level`.
    pub level: i32,
    /// `sopt_name`.
    pub name: i32,
    /// `sopt_valsize` payload as an `int`.
    pub value: u32,
}

/// The values returned by `rack_chg_query`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RackQuery {
    /// `req` (`TCP_QUERY_*`).
    pub req: i32,
    /// `req_param`.
    pub req_param: u32,
    /// `timer_hpts_flags`.
    pub timer_hpts_flags: u32,
    /// `timer_pacing_to` in microseconds.
    pub timer_pacing_to: i64,
    /// `timer_timer_exp` in microseconds.
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

impl Default for RackQuery {
    fn default() -> Self {
        Self {
            req: TCP_QUERY_TIMERS_UP,
            req_param: 0,
            timer_hpts_flags: 0,
            timer_pacing_to: 0,
            timer_timer_exp: 0,
            rack_num_dsacks: 0,
            rack_reorder_ts: 0,
            rack_rxt_last_time: 0,
            rack_min_rtt: 0,
            rack_rtt: 0,
            rack_tmit_time: 0,
            rack_srtt_measured: 0,
            rack_sacked: 0,
            rack_holes_rxt: 0,
            rack_prr_delivered: 0,
            rack_prr_recovery_fs: 0,
            rack_prr_sndcnt: 0,
            rack_prr_out: 0,
            rack_tlp_out: 0,
            rack_tlp_cnt_out: 0,
            rack_time_went_idle: 0,
            rack_in_persist: 0,
            rack_wanted_output: 0,
        }
    }
}

/// The `stack_specific_info` values filled by `rack_stack_information`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RackStackInfo {
    /// `bytes_transmitted` (`t_sndbytes`).
    pub bytes_transmitted: u64,
    /// `bytes_retransmitted` (`t_snd_rxt_bytes`).
    pub bytes_retransmitted: u64,
}

/// The RACK option state copied by `rack_inherit`.
///
/// The donor copies far more socket-option fields; this structure holds the subset the K318
/// context can represent today. Every field is copied only when the values differ, matching the
/// donor's `if (dest->x != src->x) { dest->x = src->x; cnt++; }` pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RackOptions {
    /// `dgp_on`.
    pub dgp_on: bool,
    /// `full_size_rxt`.
    pub full_size_rxt: bool,
    /// `shape_rxt_to_pacing_min`.
    pub shape_rxt_to_pacing_min: bool,
    /// `rc_rack_tmr_std_based`.
    pub rack_tmr_std_based: bool,
    /// `rc_rack_use_dsack`.
    pub rack_use_dsack: bool,
    /// `rack_hibeta`.
    pub rack_hibeta: bool,
    /// `rc_saved_beta`.
    pub saved_beta: u32,
    /// `rc_saved_beta_ecn`.
    pub saved_beta_ecn: u32,
    /// `timer_slop`.
    pub timer_slop: u32,
    /// `req_measurements`.
    pub req_measurements: u32,
    /// `r_up_only`.
    pub up_only: bool,
    /// `fillcw_cap`.
    pub fillcw_cap: u64,
    /// `bw_rate_cap`.
    pub bw_rate_cap: u64,
    /// `side_chan_dis_mask`.
    pub side_chan_dis_mask: u32,
    /// `r_limit_scw`.
    pub limit_scw: u32,
    /// `rc_pace_to_cwnd`.
    pub pace_to_cwnd: bool,
    /// `rc_pace_fill_if_rttin_range`.
    pub pace_fill_if_rttin_range: bool,
    /// `rtt_limit_mul`.
    pub rtt_limit_mul: u32,
    /// `rc_no_push_at_mrtt`.
    pub no_push_at_mrtt: bool,
    /// `rack_enable_scwnd`.
    pub enable_scwnd: bool,
    /// `rack_rec_nonrxt_use_cr`.
    pub rec_nonrxt_use_cr: bool,
    /// `rack_no_prr`.
    pub no_prr: bool,
    /// `no_prr_addback`.
    pub no_prr_addback: bool,
    /// `cspr_is_fcc`.
    pub cspr_is_fcc: bool,
    /// `rc_gp_dyn_mul`.
    pub gp_dyn_mul: bool,
    /// `rack_tlp_threshold_use`.
    pub tlp_threshold_use: u8,
    /// `init_rate`.
    pub init_rate: u64,
    /// `rc_force_max_seg`.
    pub force_max_seg: bool,
    /// `rc_user_set_min_segs`.
    pub min_segs: u16,
    /// `rc_fixed_pacing_rate_ca`.
    pub pacing_rate_ca: u64,
    /// `rc_fixed_pacing_rate_ss`.
    pub pacing_rate_ss: u64,
    /// `rc_fixed_pacing_rate_rec`.
    pub pacing_rate_rec: u64,
    /// `rack_per_of_gp_rec`.
    pub per_of_gp_rec: u32,
    /// `rack_per_of_gp_ca`.
    pub per_of_gp_ca: u32,
    /// `rack_per_of_gp_ss`.
    pub per_of_gp_ss: u32,
    /// `r_rr_config`.
    pub rr_config: u8,
    /// `r_use_cmp_ack`.
    pub use_cmp_ack: bool,
    /// `r_mbuf_queue`.
    pub mbuf_queue: bool,
}

impl Default for RackOptions {
    fn default() -> Self {
        Self {
            dgp_on: false,
            full_size_rxt: false,
            shape_rxt_to_pacing_min: false,
            rack_tmr_std_based: false,
            rack_use_dsack: false,
            rack_hibeta: false,
            saved_beta: 50,
            saved_beta_ecn: 50,
            timer_slop: 0,
            req_measurements: 0,
            up_only: false,
            fillcw_cap: 0,
            bw_rate_cap: 0,
            side_chan_dis_mask: 0,
            limit_scw: 0,
            pace_to_cwnd: false,
            pace_fill_if_rttin_range: false,
            rtt_limit_mul: 0,
            no_push_at_mrtt: false,
            enable_scwnd: false,
            rec_nonrxt_use_cr: false,
            no_prr: false,
            no_prr_addback: false,
            cspr_is_fcc: false,
            gp_dyn_mul: false,
            tlp_threshold_use: 0,
            init_rate: 0,
            force_max_seg: false,
            min_segs: 0,
            pacing_rate_ca: 0,
            pacing_rate_ss: 0,
            pacing_rate_rec: 0,
            per_of_gp_rec: 0,
            per_of_gp_ca: 0,
            per_of_gp_ss: 0,
            rr_config: 0,
            use_cmp_ack: false,
            mbuf_queue: false,
        }
    }
}

/// The `tcp_function_block` callback set used by `rack.c`.
pub trait RackVtable {
    /// `tfb_tcp_output`: transmit queued data or a retransmission.
    fn tfb_tcp_output(&mut self) -> Result<(), Errno>;
    /// `tfb_tcp_do_segment`: drain queued input, then process one segment.
    fn tfb_tcp_do_segment(&mut self, segment: &RackSegment);
    /// `tfb_do_segment_nounlock`: process one segment without the lock.
    fn tfb_do_segment_nounlock(
        &mut self,
        segment: &RackSegment,
    ) -> Result<RackSegmentResult, Errno>;
    /// `tfb_do_queued_segments`: drain the HPTS input queue; `true` means the tcb was lost.
    fn tfb_do_queued_segments(&mut self, have_pkt: bool) -> bool;
    /// `tfb_tcp_ctloutput`: get/set a rack socket option.
    fn tfb_tcp_ctloutput(&mut self, option: &RackSockopt) -> Result<Option<u32>, Errno>;
    /// `tfb_tcp_fb_init`: allocate the RACK state.
    fn tfb_tcp_fb_init(&mut self) -> Result<(), Errno>;
    /// `tfb_tcp_fb_fini`: free the RACK state.
    fn tfb_tcp_fb_fini(&mut self, tcb_is_purged: bool);
    /// `tfb_tcp_timer_stop_all`: mark the timers stopped and disarm HPTS.
    fn tfb_tcp_timer_stop_all(&mut self) -> Result<(), Errno>;
    /// `tfb_tcp_rexmit_tmr`: the retransmit timer event.
    fn tfb_tcp_rexmit_tmr(&mut self);
    /// `tfb_tcp_handoff_ok`: whether the stack may be handed off.
    fn tfb_tcp_handoff_ok(&self) -> Result<(), Errno>;
    /// `tfb_tcp_mtu_chg`: apply a path MTU change (the caller already updated `t_maxseg`).
    fn tfb_tcp_mtu_chg(&mut self);
    /// `tfb_pru_options`: validate a protocol-user option.
    fn tfb_pru_options(&self, flags: i32) -> Result<(), Errno>;
    /// `tfb_hwtls_change`: record the hardware KTLS enable/disable.
    fn tfb_hwtls_change(&mut self, enable: bool);
    /// `tfb_chg_query`: answer a `struct tcp_query_resp`.
    fn tfb_chg_query(&mut self, query: &mut RackQuery) -> i32;
    /// `tfb_switch_failed`: restore the HPTS/t_flags2 state after a failed stack switch.
    fn tfb_switch_failed(&mut self);
    /// `tfb_early_wake_check`: whether an HPTS timer is due.
    fn tfb_early_wake_check(&self) -> bool;
    /// `tfb_compute_pipe`: the in-flight bytes (`do_rack_compute_pipe`).
    fn tfb_compute_pipe(&self) -> i32;
    /// `tfb_stack_info`: the stack summary.
    fn tfb_stack_info(&self) -> RackStackInfo;
    /// `tfb_inherit`: inherit the socket-option state from a parent stack.
    fn tfb_inherit(&mut self, source: &Self)
    where
        Self: Sized;
}

/// A RACK transport stack over the K314 context.
pub struct RackStack {
    /// The installed function block.
    pub fb: RackFunctionBlock,
    /// K314 buffers/zones/sysctl/log/KTLS/pacing.
    pub os: RackOs,
    /// The tcpcb mirror.
    pub tp: RackTcpcb,
    /// Lifecycle.
    pub state: RackPortState,
    /// UMA zone for `tcp_rack`.
    pub zone: Option<u32>,
    /// Allocated `tcp_rack` item (`t_fb_ptr`).
    pub fb_item: Option<u64>,
    /// Hardware KTLS flag (`rack->r_ctl.fsb.hw_tls`).
    pub hw_tls: bool,
    /// Queued input segments (`t_inqueue`).
    pub queue: VecDeque<RackSegment>,
    /// `tcb_is_purged` from the last `tfb_tcp_fb_fini`.
    pub tcb_is_purged: bool,
    /// Last event marker.
    pub last_event: Option<u32>,
    /// Socket-option state inherited by `rack_inherit`.
    pub options: RackOptions,
    /// `t_flags2`.
    pub t_flags2: u32,
    /// `rack->t_timers_stopped`.
    pub t_timers_stopped: bool,
    /// `rack->r_ctl.rc_hpts_flags`.
    pub hpts_flags: u32,
    /// `rack->r_ctl.rc_last_output_to` in microseconds.
    pub last_output_to: i64,
    /// `rack->r_ctl.rc_timer_exp` in microseconds.
    pub timer_exp: i64,
    /// `rack->r_timer_override`.
    pub timer_override: bool,
    /// `rack->r_ctl.rc_snd_max_at_rto`.
    pub snd_max_at_rto: u32,
    /// `rack->r_ctl.rc_last_timeout_snduna`.
    pub last_timeout_snduna: u32,
    /// `rack->r_must_retran`.
    pub must_retran: bool,
    /// `rack->r_ctl.rc_tmr_stopped`.
    pub tmr_stopped: u32,
    /// `rack->r_ctl.rc_pace_min_segs`.
    pub pace_min_segs: u16,
    /// `rack_sack_not_required`.
    pub sack_not_required: bool,
    /// `tp->t_cc->name == "newreno"` for the beta options.
    pub cc_is_newreno: bool,
    /// `((struct newreno *)cc_data)->beta`.
    pub cc_beta: u32,
    /// `rack->rc_pace_dnd`.
    pub pacing_dnd: bool,
    /// `tp->snd_wnd`.
    pub snd_wnd: u32,
    /// `tp->t_dupacks`.
    pub dupacks: u16,
    /// `rack->r_ctl.rc_sacked`.
    pub sacked: u32,
    /// `rack->r_ctl.rc_considered_lost`.
    pub considered_lost: u32,
    /// `rack->r_ctl.rc_holes_rxt`.
    pub holes_rxt: u32,
    /// `rack->r_ctl.num_dsack`.
    pub num_dsacks: u32,
    /// `rack->r_ctl.rc_reorder_ts`.
    pub reorder_ts: u32,
    /// `rack->r_ctl.rc_tlp_rxt_last_time`.
    pub tlp_rxt_last_time: u32,
    /// `rack->r_ctl.rc_rack_min_rtt`.
    pub rack_min_rtt: u32,
    /// `rack->rc_rack_rtt`.
    pub rack_rtt: u32,
    /// `rack->r_ctl.rc_rack_tmit_time`.
    pub rack_tmit_time: u32,
    /// `rack->rc_srtt_measure_made`.
    pub srtt_measured: u32,
    /// `rack->r_ctl.rc_prr_delivered`.
    pub prr_delivered: u32,
    /// `rack->r_ctl.rc_prr_recovery_fs`.
    pub prr_recovery_fs: u32,
    /// `rack->r_ctl.rc_prr_sndcnt`.
    pub prr_sndcnt: u32,
    /// `rack->r_ctl.rc_prr_out`.
    pub prr_out: u32,
    /// `rack->rc_tlp_in_progress`.
    pub tlp_out: bool,
    /// `rack->r_ctl.rc_tlp_cnt_out`.
    pub tlp_cnt_out: u32,
    /// `rack->rc_in_persist`.
    pub in_persist: bool,
    /// `rack->r_ctl.rc_went_idle_time`.
    pub went_idle_time: u32,
    /// `rack->r_wanted_output`.
    pub wanted_output: bool,
    /// `tp->t_sndbytes`.
    pub bytes_transmitted: u64,
    /// `tp->t_snd_rxt_bytes`.
    pub bytes_retransmitted: u64,
}

impl Default for RackStack {
    fn default() -> Self {
        Self::new()
    }
}

impl RackStack {
    /// Create an uninitialized RACK stack.
    #[must_use]
    pub fn new() -> Self {
        Self {
            fb: RackFunctionBlock::rack(),
            os: RackOs::new(),
            tp: RackTcpcb::default(),
            state: RackPortState::Uninitialized,
            zone: None,
            fb_item: None,
            hw_tls: false,
            queue: VecDeque::new(),
            tcb_is_purged: false,
            last_event: None,
            options: RackOptions::default(),
            t_flags2: 0,
            t_timers_stopped: false,
            hpts_flags: 0,
            last_output_to: 0,
            timer_exp: 0,
            timer_override: false,
            snd_max_at_rto: 0,
            last_timeout_snduna: 0,
            must_retran: false,
            tmr_stopped: 0,
            pace_min_segs: 0,
            sack_not_required: false,
            cc_is_newreno: true,
            cc_beta: 50,
            pacing_dnd: false,
            snd_wnd: 65_535,
            dupacks: 0,
            sacked: 0,
            considered_lost: 0,
            holes_rxt: 0,
            num_dsacks: 0,
            reorder_ts: 0,
            tlp_rxt_last_time: 0,
            rack_min_rtt: 0,
            rack_rtt: 0,
            rack_tmit_time: 0,
            srtt_measured: 0,
            prr_delivered: 0,
            prr_recovery_fs: 0,
            prr_sndcnt: 0,
            prr_out: 0,
            tlp_out: false,
            tlp_cnt_out: 0,
            in_persist: false,
            went_idle_time: 0,
            wanted_output: false,
            bytes_transmitted: 0,
            bytes_retransmitted: 0,
        }
    }

    /// Queue one input segment for `tfb_do_queued_segments`/`tfb_tcp_do_segment`.
    pub fn enqueue(&mut self, segment: RackSegment) {
        self.queue.push_back(segment);
    }

    fn log(&mut self, event: u32) {
        self.last_event = Some(event);
        let _ = self.os.tcp_log_event(
            event,
            [
                self.tp.snd_una,
                self.tp.snd_nxt,
                u32::from(self.tp.t_state),
                0,
            ],
            0,
        );
    }

    /// `rack_timer_cancel`: remove the HPTS entry and clear the due timer bits.
    fn rack_timer_cancel(&mut self) {
        let flags = self.hpts_flags;
        let outstanding = self.tp.snd_max.wrapping_sub(self.tp.snd_una);
        if flags & PACE_PKT_OUTPUT != 0
            && (self.os.now_us >= self.last_output_to || outstanding == 0)
        {
            let _ = self.os.tcp_hpts_remove();
            if outstanding == 0 {
                self.hpts_flags &= !PACE_PKT_OUTPUT;
            }
        }
        if self.hpts_flags & PACE_TMR_MASK != 0 {
            self.tmr_stopped = self.hpts_flags & PACE_TMR_MASK;
            if self.os.pacing.hpts.active && self.hpts_flags & PACE_PKT_OUTPUT == 0 {
                let _ = self.os.tcp_hpts_remove();
            }
            self.hpts_flags &= !PACE_TMR_MASK;
        }
    }

    /// Record an HPTS insert and its due flags.
    fn hpts_insert(&mut self, usecs: u32, flags: u32) {
        let deadline = self.os.tcp_hpts_insert(usecs);
        self.hpts_flags |= flags;
        if flags & PACE_PKT_OUTPUT != 0 {
            self.last_output_to = deadline;
        }
        if flags & PACE_TMR_MASK != 0 {
            self.timer_exp = deadline;
        }
    }

    /// Transmit one retransmission chunk if `r_must_retran` is set.
    fn retransmit_chunk(&mut self) -> bool {
        if !self.must_retran {
            return false;
        }
        if self.tp.snd_nxt == self.tp.snd_max && seq_gt(self.tp.snd_max, self.tp.snd_una) {
            self.tp.snd_nxt = self.tp.snd_una;
        }
        let limit = if self.snd_max_at_rto != 0 {
            self.snd_max_at_rto
        } else {
            self.tp.snd_max
        };
        if !seq_lt(self.tp.snd_nxt, limit) {
            self.must_retran = false;
            self.snd_max_at_rto = 0;
            return false;
        }
        let len = limit
            .wrapping_sub(self.tp.snd_nxt)
            .min(u32::from(self.tp.t_maxseg));
        if len == 0 {
            return false;
        }
        self.tp.snd_nxt = self.tp.snd_nxt.wrapping_add(len);
        self.bytes_retransmitted = self.bytes_retransmitted.saturating_add(u64::from(len));
        if seq_ge(self.tp.snd_nxt, limit) {
            self.must_retran = false;
            self.snd_max_at_rto = 0;
        }
        self.wanted_output = false;
        self.log(0x101);
        true
    }

    /// `rack_set_sockopt` for the K318 option subset.
    fn rack_set_sockopt(&mut self, option: &RackSockopt) -> Result<Option<u32>, Errno> {
        if option.level != IPPROTO_TCP {
            return Err(Errno::EOPNOTSUPP);
        }
        match option.name {
            TCP_RACK_PACE_MIN_SEG => {
                self.os.pacing.min_seg = u16::try_from(option.value & 0xffff).unwrap_or(u16::MAX);
                self.options.min_segs = self.os.pacing.min_seg;
                Ok(None)
            }
            TCP_RACK_PACE_MAX_SEG => {
                let value = option.value.min(MAX_USER_SET_SEG);
                self.os.pacing.max_segs = u16::try_from(value).unwrap_or(u16::MAX);
                Ok(None)
            }
            TCP_RACK_PACING_DIVISOR => {
                self.os.pacing.divisor = if option.value == 0 {
                    RACK_DEFAULT_PACING_DIVISOR
                } else {
                    option.value.max(RL_MIN_DIVISOR)
                };
                Ok(None)
            }
            TCP_PACING_DND => {
                self.pacing_dnd = option.value > 0;
                Ok(None)
            }
            TCP_RACK_PACING_BETA => Err(Errno::EINVAL),
            _ => Err(Errno::EOPNOTSUPP),
        }
    }

    /// `rack_get_sockopt` for the K318 option subset.
    fn rack_get_sockopt(&self, option: &RackSockopt) -> Result<Option<u32>, Errno> {
        if option.level != IPPROTO_TCP {
            return Err(Errno::EOPNOTSUPP);
        }
        match option.name {
            TCP_RACK_PACE_MIN_SEG => Ok(Some(u32::from(self.os.pacing.min_seg))),
            TCP_RACK_PACE_MAX_SEG => Ok(Some(u32::from(self.os.pacing.max_segs))),
            TCP_RACK_PACING_DIVISOR => Ok(Some(self.os.pacing.divisor)),
            TCP_PACING_DND => Ok(Some(u32::from(self.pacing_dnd))),
            TCP_RACK_PACING_BETA => {
                if !self.cc_is_newreno {
                    return Err(Errno::EINVAL);
                }
                if self.os.pacing.cc_pacing {
                    Ok(Some(self.cc_beta))
                } else {
                    Ok(Some(self.options.saved_beta))
                }
            }
            _ => Err(Errno::EOPNOTSUPP),
        }
    }

    /// `rack_stack_information` fills the counters.
    #[must_use]
    pub fn stack_info(&self) -> RackStackInfo {
        RackStackInfo {
            bytes_transmitted: self.bytes_transmitted,
            bytes_retransmitted: self.bytes_retransmitted,
        }
    }
}

impl RackVtable for RackStack {
    fn tfb_tcp_output(&mut self) -> Result<(), Errno> {
        if self.state != RackPortState::Running {
            return Err(Errno::ENOTCONN);
        }
        // `rack_output`: `rack->r_ctl.rc_resend`/`tcp_rack_output` retransmissions take precedence.
        if self.retransmit_chunk() {
            return Ok(());
        }
        // New data: the donor takes it at `snd_max`; the send buffer is cut only by ACK processing.
        let offset = self.tp.snd_max.wrapping_sub(self.tp.snd_una);
        let sbavail = self.os.sbavail();
        if sbavail <= offset {
            self.wanted_output = false;
            return Ok(());
        }
        let flight = offset;
        let cwnd = self.tp.snd_cwnd;
        if cwnd <= flight {
            return Ok(());
        }
        let len = (sbavail - offset)
            .min(u32::from(self.tp.t_maxseg))
            .min(cwnd - flight);
        if len == 0 {
            return Ok(());
        }
        self.tp.snd_max = self.tp.snd_max.wrapping_add(len);
        if seq_gt(self.tp.snd_max, self.tp.snd_nxt) {
            self.tp.snd_nxt = self.tp.snd_max;
        }
        self.tp.t_sndtime = self.os.tcp_ts_getticks();
        self.bytes_transmitted = self.bytes_transmitted.saturating_add(u64::from(len));
        self.wanted_output = false;
        if self.os.pacing.enabled || self.os.pacing.method != PacingMethod::None {
            self.hpts_insert(HPTS_USECS_PER_SLOT, PACE_PKT_OUTPUT);
        }
        self.log(0x100);
        Ok(())
    }

    fn tfb_tcp_do_segment(&mut self, segment: &RackSegment) {
        if !self.queue.is_empty() && self.tfb_do_queued_segments(true) {
            return;
        }
        let _ = self.tfb_do_segment_nounlock(segment);
    }

    fn tfb_do_segment_nounlock(
        &mut self,
        segment: &RackSegment,
    ) -> Result<RackSegmentResult, Errno> {
        if self.state != RackPortState::Running {
            return Err(Errno::ENOTCONN);
        }
        if segment.flags & TH_RST != 0 {
            self.tp.t_state = TCPS_CLOSED;
            self.must_retran = false;
            self.wanted_output = false;
            self.log(0x201);
            return Ok(RackSegmentResult::Reset);
        }
        if segment.flags & TH_ACK != 0 {
            if !seq_gt(segment.ack, self.tp.snd_una) {
                self.dupacks = self.dupacks.saturating_add(1);
                self.log(0x203);
                return Ok(RackSegmentResult::DuplicateAck);
            }
            if seq_gt(segment.ack, self.tp.snd_max) {
                // Acknowledge only what was sent; the donor challenges the ACK.
                self.log(0x204);
                return Ok(RackSegmentResult::Ignored);
            }
            let acked = segment
                .ack
                .wrapping_sub(self.tp.snd_una)
                .min(self.os.sbused());
            if acked > 0 {
                let cut = self.os.sbcut_locked(acked as usize);
                self.os.m_freem(cut);
                self.tp.snd_una = segment.ack;
                self.tp.t_rxtshift = 0;
                self.tp.t_rxtcur = self.tp.rack_rexmtval();
                self.tlp_out = false;
                self.tlp_cnt_out = 0;
                if self.hpts_flags & PACE_TMR_RXT != 0 {
                    self.rack_timer_cancel();
                }
            } else {
                self.tp.snd_una = segment.ack;
            }
            self.snd_wnd = u32::from(segment.window);
            self.dupacks = 0;
            if self.must_retran && seq_ge(segment.ack, self.snd_max_at_rto) {
                self.must_retran = false;
                self.snd_max_at_rto = 0;
            }
            if self.tp.snd_una == self.tp.snd_max {
                if self.os.sbavail() == 0 {
                    self.tp.t_acktime = 0;
                }
                self.rack_timer_cancel();
            }
            if self.os.sbavail() > 0 {
                self.wanted_output = true;
            }
            self.log(0x200);
            return Ok(RackSegmentResult::Acked(acked));
        }
        if segment.flags & TH_FIN != 0 {
            if self.tp.t_state == TCPS_ESTABLISHED {
                self.tp.t_state = TCPS_CLOSE_WAIT;
            }
            self.log(0x202);
            return Ok(RackSegmentResult::Fin);
        }
        if segment.payload_len > 0 {
            self.tp.t_rcvtime = self.os.tcp_ts_getticks();
            self.log(0x205);
            return Ok(RackSegmentResult::Data);
        }
        Ok(RackSegmentResult::Ignored)
    }

    fn tfb_do_queued_segments(&mut self, _have_pkt: bool) -> bool {
        let mut lost = false;
        while let Some(segment) = self.queue.pop_front() {
            match self.tfb_do_segment_nounlock(&segment) {
                Ok(RackSegmentResult::Reset) | Err(_) => {
                    lost = true;
                    break;
                }
                Ok(_) => {}
            }
        }
        if lost {
            self.queue.clear();
        }
        lost
    }

    fn tfb_tcp_ctloutput(&mut self, option: &RackSockopt) -> Result<Option<u32>, Errno> {
        match option.dir {
            SockoptDir::Set => self.rack_set_sockopt(option),
            SockoptDir::Get => self.rack_get_sockopt(option),
        }
    }

    fn tfb_tcp_fb_init(&mut self) -> Result<(), Errno> {
        if self.state == RackPortState::Running {
            return Err(Errno::EINVAL);
        }
        let zone = match self.zone {
            Some(zone) => zone,
            None => {
                let zone = self.os.uma_zcreate("tcp_rack", 128);
                let _ = self.os.uma_zone_set_max(zone, 64);
                self.zone = Some(zone);
                zone
            }
        };
        let item = self.os.uma_zalloc(zone).ok_or(Errno::ENOMEM)?;
        self.fb_item = Some(item);
        self.state = RackPortState::Running;
        self.pace_min_segs = self.tp.t_maxseg;
        self.options = RackOptions::default();
        self.os.pacing.divisor = RACK_DEFAULT_PACING_DIVISOR;
        self.os.pacing.min_seg = 0;
        self.os.pacing.max_segs = 40;
        self.log(0x001);
        Ok(())
    }

    fn tfb_tcp_fb_fini(&mut self, tcb_is_purged: bool) {
        if self.fb_item.is_none() {
            return;
        }
        self.os.rack_remove_pacing();
        let _ = self.os.log.tcp_log_flowend();
        if let (Some(zone), Some(item)) = (self.zone.take(), self.fb_item.take()) {
            let _ = self.os.uma_zfree(zone, item);
            let _ = self.os.uma_zdestroy(zone);
        }
        self.state = RackPortState::Fini;
        self.tcb_is_purged = tcb_is_purged;
        self.hw_tls = false;
        self.hpts_flags = 0;
        self.log(0x002);
    }

    fn tfb_tcp_timer_stop_all(&mut self) -> Result<(), Errno> {
        self.t_timers_stopped = true;
        let _ = self.os.tcp_hpts_remove();
        self.hpts_flags = 0;
        self.log(0x010);
        Ok(())
    }

    fn tfb_tcp_rexmit_tmr(&mut self) {
        self.rack_timer_cancel();
        self.timer_override = true;
        self.snd_max_at_rto = self.tp.snd_max;
        self.last_timeout_snduna = self.tp.snd_una;
        self.tmr_stopped = 0;
        if self.tp.t_rxtshift <= RACK_RXT_SCOREBOARD_CLEAR_THRESH {
            self.must_retran = seq_gt(self.tp.snd_max, self.tp.snd_una);
        } else {
            // The donor clears the scoreboard after too many time-outs.
            self.must_retran = false;
            self.sacked = 0;
        }
        self.wanted_output = true;
        self.log(0x011);
    }

    fn tfb_tcp_handoff_ok(&self) -> Result<(), Errno> {
        if self.state != RackPortState::Running {
            return Err(Errno::EINVAL);
        }
        match self.tp.t_state {
            TCPS_CLOSED | TCPS_LISTEN => return Ok(()),
            TCPS_SYN_SENT | TCPS_SYN_RECEIVED => return Err(Errno::EAGAIN),
            _ => {}
        }
        if self.tp.t_flags & TF_SENTFIN != 0 && self.tp.snd_max.wrapping_sub(self.tp.snd_una) > 1 {
            return Err(Errno::EAGAIN);
        }
        if self.tp.t_flags & TF_SACK_PERMIT != 0 || self.sack_not_required {
            Ok(())
        } else {
            Err(Errno::EINVAL)
        }
    }

    fn tfb_tcp_mtu_chg(&mut self) {
        // `rack_mtu_change`: the caller has already updated `t_maxseg`.
        let new_mss = self.tp.t_maxseg;
        if self.pace_min_segs != new_mss {
            self.pace_min_segs = new_mss;
            self.tfb_tcp_rexmit_tmr();
            self.must_retran = true;
        }
        if seq_gt(self.tp.snd_max, self.tp.snd_nxt) {
            self.tp.snd_nxt = self.tp.snd_max;
        }
        self.log(0x020);
    }

    fn tfb_pru_options(&self, flags: i32) -> Result<(), Errno> {
        if flags & PRUS_OOB != 0 {
            Err(Errno::EOPNOTSUPP)
        } else {
            Ok(())
        }
    }

    fn tfb_hwtls_change(&mut self, enable: bool) {
        self.hw_tls = enable;
        self.log(0x030);
    }

    fn tfb_chg_query(&mut self, query: &mut RackQuery) -> i32 {
        match query.req {
            TCP_QUERY_SENDMAP => {
                // The sendmap (`tqhash`) is not part of the K314 context; the donor returns 0
                // when the sequence is not found, which is the model's answer here.
                self.log(0x041);
                0
            }
            TCP_QUERY_TIMERS_UP => {
                if self.hpts_flags == 0 {
                    return 0;
                }
                query.timer_hpts_flags = self.hpts_flags;
                if self.hpts_flags & PACE_PKT_OUTPUT != 0 {
                    query.timer_pacing_to = self.last_output_to;
                }
                if self.hpts_flags & PACE_TMR_MASK != 0 {
                    query.timer_timer_exp = self.timer_exp;
                }
                self.log(0x042);
                1
            }
            TCP_QUERY_RACK_TIMES => {
                query.rack_num_dsacks = self.num_dsacks;
                query.rack_reorder_ts = self.reorder_ts;
                query.rack_rxt_last_time = self.tlp_rxt_last_time;
                query.rack_min_rtt = self.rack_min_rtt;
                query.rack_rtt = self.rack_rtt;
                query.rack_tmit_time = self.rack_tmit_time;
                query.rack_srtt_measured = self.srtt_measured;
                query.rack_sacked = self.sacked;
                query.rack_holes_rxt = self.holes_rxt;
                query.rack_prr_delivered = self.prr_delivered;
                query.rack_prr_recovery_fs = self.prr_recovery_fs;
                query.rack_prr_sndcnt = self.prr_sndcnt;
                query.rack_prr_out = self.prr_out;
                query.rack_tlp_out = u8::from(self.tlp_out);
                query.rack_tlp_cnt_out = self.tlp_cnt_out;
                if self.in_persist {
                    query.rack_time_went_idle = self.went_idle_time;
                    query.rack_in_persist = 1;
                } else {
                    query.rack_time_went_idle = 0;
                    query.rack_in_persist = 0;
                }
                query.rack_wanted_output = u8::from(self.wanted_output);
                self.log(0x043);
                1
            }
            _ => i32::try_from(Errno::EINVAL.to_ret()).unwrap_or(-22),
        }
    }

    fn tfb_switch_failed(&mut self) {
        // `tcp_change_time_units(tp, TCP_TMR_GRANULARITY_USEC)` is the model's microsecond clock.
        if self.options.mbuf_queue || self.os.pacing.enabled || self.options.use_cmp_ack {
            self.t_flags2 |= TF2_SUPPORTS_MBUFQ;
        } else {
            self.t_flags2 &= !TF2_SUPPORTS_MBUFQ;
        }
        if self.options.use_cmp_ack && self.tp.t_state >= TCPS_ESTABLISHED {
            self.t_flags2 |= TF2_MBUF_ACKCMP;
        }
        if self.os.pacing.hpts.active {
            return;
        }
        let now = self.os.now_us;
        let toval = if self.hpts_flags & PACE_PKT_OUTPUT != 0 {
            if self.last_output_to > now {
                u32::try_from(self.last_output_to - now).unwrap_or(HPTS_USECS_PER_SLOT)
            } else {
                HPTS_USECS_PER_SLOT
            }
        } else if self.hpts_flags & PACE_TMR_MASK != 0 {
            if self.timer_exp > now {
                u32::try_from(self.timer_exp - now).unwrap_or(HPTS_USECS_PER_SLOT)
            } else {
                HPTS_USECS_PER_SLOT
            }
        } else {
            HPTS_USECS_PER_SLOT
        };
        self.hpts_insert(toval, 0);
        self.log(0x040);
    }

    fn tfb_early_wake_check(&self) -> bool {
        if self.hpts_flags == 0 {
            return false;
        }
        let now = self.os.now_us;
        if self.hpts_flags & PACE_PKT_OUTPUT != 0 {
            now >= self.last_output_to
        } else if self.hpts_flags & PACE_TMR_MASK != 0 {
            now >= self.timer_exp
        } else {
            false
        }
    }

    fn tfb_compute_pipe(&self) -> i32 {
        let outstanding = self.tp.snd_max.wrapping_sub(self.tp.snd_una);
        let removed = self.sacked.saturating_add(self.considered_lost);
        let pipe = outstanding
            .saturating_sub(removed)
            .saturating_add(self.holes_rxt);
        i32::try_from(pipe).unwrap_or(i32::MAX)
    }

    fn tfb_stack_info(&self) -> RackStackInfo {
        self.stack_info()
    }

    fn tfb_inherit(&mut self, source: &Self) {
        if self.fb_item.is_none() || source.fb_item.is_none() {
            self.log(0x051);
            return;
        }
        if self.options != source.options {
            self.options = source.options.clone();
        }
        if self.os.pacing.divisor != source.os.pacing.divisor {
            self.os.pacing.divisor = source.os.pacing.divisor;
        }
        if self.options.mbuf_queue || self.os.pacing.enabled || self.options.use_cmp_ack {
            self.t_flags2 |= TF2_SUPPORTS_MBUFQ;
        } else {
            self.t_flags2 &= !TF2_SUPPORTS_MBUFQ;
        }
        if self.options.use_cmp_ack && self.tp.t_state >= TCPS_ESTABLISHED {
            self.t_flags2 |= TF2_MBUF_ACKCMP;
        }
        self.log(0x050);
    }
}

#[cfg(test)]
#[path = "rack_vtable_ut.rs"]
mod tests;
