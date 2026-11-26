// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent model of the FreeBSD `tcp_function_block` callbacks installed by rack.c.
//!
//! The model is deliberately not the RACK core: K319/K320 own loss, TLP, DSACK and recovery.
//! This crate records the vtable lifecycle, callback order/lifetimes/returns and the named limits
//! so the future `rack_vtable.rs` implementation can be compared with an independent trace.

use std::string::String;

/// `EINVAL` on the model's return ABI.
pub const EINVAL: i32 = 22;
/// `EAGAIN` on the model's return ABI.
pub const EAGAIN: i32 = 11;
/// `EOPNOTSUPP` on the model's return ABI.
pub const EOPNOTSUPP: i32 = 95;
/// `ENOMEM` on the model's return ABI.
pub const ENOMEM: i32 = 12;
/// `ENOSPC` on the model's return ABI.
pub const ENOSPC: i32 = 28;

/// FreeBSD `PACE_TMR_DELACK`.
pub const PACE_TMR_DELACK: u8 = 0x01;
/// FreeBSD `PACE_TMR_RACK`.
pub const PACE_TMR_RACK: u8 = 0x02;
/// FreeBSD `PACE_TMR_TLP`.
pub const PACE_TMR_TLP: u8 = 0x04;
/// FreeBSD `PACE_TMR_RXT`.
pub const PACE_TMR_RXT: u8 = 0x08;
/// FreeBSD `PACE_TMR_PERSIT`.
pub const PACE_TMR_PERSIT: u8 = 0x10;
/// FreeBSD `PACE_TMR_KEEP`.
pub const PACE_TMR_KEEP: u8 = 0x20;
/// FreeBSD `PACE_PKT_OUTPUT`.
pub const PACE_PKT_OUTPUT: u8 = 0x40;
/// FreeBSD `PACE_TMR_MASK`.
pub const PACE_TMR_MASK: u8 =
    PACE_TMR_KEEP | PACE_TMR_PERSIT | PACE_TMR_RXT | PACE_TMR_TLP | PACE_TMR_RACK | PACE_TMR_DELACK;

/// `TCP_FUNC_OUTPUT_CANDROP`.
pub const TCP_FUNC_OUTPUT_CANDROP: u32 = 0x02;
/// `TCP_FUNC_DEFAULT_OK`.
pub const TCP_FUNC_DEFAULT_OK: u32 = 0x04;
/// The flags rack's `__tcp_rack` installs.
pub const RACK_FB_FLAGS: u32 = TCP_FUNC_OUTPUT_CANDROP | TCP_FUNC_DEFAULT_OK;
/// FreeBSD `TF2_MBUF_ACKCMP`.
pub const TF2_MBUF_ACKCMP: u32 = 0x0000_8000;
/// FreeBSD `TF2_SUPPORTS_MBUFQ`.
pub const TF2_SUPPORTS_MBUFQ: u32 = 0x0001_0000;
/// `HPTS_USECS_PER_SLOT`.
pub const HPTS_USECS_PER_SLOT: u64 = 10;
/// Default `rack_default_pacing_divisor`.
pub const DEFAULT_PACING_DIVISOR: u32 = 250;
/// `RL_MIN_DIVISOR`.
pub const RL_MIN_DIVISOR: u32 = 50;
/// `rack_rxt_scoreboard_clear_thresh`.
pub const RXT_SCOREBOARD_CLEAR_THRESH: u8 = 2;
/// `PRUS_OOB` (the only option rack rejects).
pub const PRUS_OOB: u32 = 0x01;

/// FreeBSD `TCP_RACK_PACE_MAX_SEG`.
pub const TCP_RACK_PACE_MAX_SEG: u32 = 1054;
/// FreeBSD `TCP_RACK_PACING_DIVISOR`.
pub const TCP_RACK_PACING_DIVISOR: u32 = 1146;
/// FreeBSD `TCP_RACK_PACE_MIN_SEG`.
pub const TCP_RACK_PACE_MIN_SEG: u32 = 1147;
/// FreeBSD `TCP_RACK_PACING_BETA`.
pub const TCP_RACK_PACING_BETA: u32 = 1138;
/// FreeBSD `TCP_RACK_PACING_BETA_ECN`.
pub const TCP_RACK_PACING_BETA_ECN: u32 = 1139;

/// `TCP_QUERY_SENDMAP`.
pub const TCP_QUERY_SENDMAP: u32 = 1;
/// `TCP_QUERY_TIMERS_UP`.
pub const TCP_QUERY_TIMERS_UP: u32 = 2;
/// `TCP_QUERY_RACK_TIMES`.
pub const TCP_QUERY_RACK_TIMES: u32 = 3;

/// The connection state relevant to the vtable contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TcpState {
    /// Closed.
    Closed,
    /// Listening.
    Listen,
    /// SYN sent.
    SynSent,
    /// SYN received.
    SynReceived,
    /// Established.
    Established,
    /// FIN sent.
    FinWait1,
    /// FIN acknowledged.
    FinWait2,
    /// Peer FIN received.
    CloseWait,
    /// Both FINs.
    Closing,
    /// Peer FIN acknowledged.
    LastAck,
    /// Time-wait.
    TimeWait,
}

/// The segment class passed to the input callbacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    /// RST.
    Rst,
    /// Pure ACK.
    Ack,
    /// Data.
    Data,
    /// FIN.
    Fin,
}

/// Socket-option direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SockoptDir {
    /// Set.
    Set,
    /// Get.
    Get,
}

/// The rack socket options covered by the vtable model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackOption {
    /// Maximum segment size for a pace.
    PaceMaxSeg,
    /// Minimum segment size for a pace.
    PaceMinSeg,
    /// Pacing divisor.
    PacingDivisor,
    /// NewReno pacing beta.
    PacingBeta,
    /// NewReno ECN pacing beta.
    PacingBetaEcn,
    /// Unknown option number.
    Unknown(u32),
}

impl RackOption {
    /// Map a donor option number.
    #[must_use]
    pub const fn from_u32(value: u32) -> Self {
        match value {
            TCP_RACK_PACE_MAX_SEG => Self::PaceMaxSeg,
            TCP_RACK_PACE_MIN_SEG => Self::PaceMinSeg,
            TCP_RACK_PACING_DIVISOR => Self::PacingDivisor,
            TCP_RACK_PACING_BETA => Self::PacingBeta,
            TCP_RACK_PACING_BETA_ECN => Self::PacingBetaEcn,
            other => Self::Unknown(other),
        }
    }
}

/// A query type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Query {
    /// Send-map query.
    SendMap,
    /// Timers query.
    TimersUp,
    /// RACK timing query.
    RackTimes,
    /// Unknown query.
    Unknown(u32),
}

impl Query {
    /// Map a donor query number.
    #[must_use]
    pub const fn from_u32(value: u32) -> Self {
        match value {
            TCP_QUERY_SENDMAP => Self::SendMap,
            TCP_QUERY_TIMERS_UP => Self::TimersUp,
            TCP_QUERY_RACK_TIMES => Self::RackTimes,
            other => Self::Unknown(other),
        }
    }
}

/// The inherited option/flag subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RackOptions {
    /// `rc_user_set_min_segs`.
    pub pace_min_segs: u16,
    /// `rc_user_set_max_segs`.
    pub pace_max_segs: u16,
    /// `pace_len_divisor`.
    pub pacing_divisor: u32,
    /// NewReno pacing beta.
    pub beta: u32,
    /// NewReno ECN pacing beta.
    pub beta_ecn: u32,
    /// The congestion-control name is newreno.
    pub newreno: bool,
    /// The `t_flags2` tail copied by `rack_inherit`.
    pub flags2: u32,
}

impl Default for RackOptions {
    fn default() -> Self {
        Self {
            pace_min_segs: 0,
            pace_max_segs: 0,
            pacing_divisor: DEFAULT_PACING_DIVISOR,
            beta: 50,
            beta_ecn: 80,
            newreno: true,
            flags2: 0,
        }
    }
}

/// A control-callback reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CtlReply {
    /// Error code.
    pub error: i32,
    /// Returned value for a get.
    pub value: Option<u32>,
}

/// A query reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryReply {
    /// Error code.
    pub error: i32,
    /// Query result (0/1), or `-EINVAL`.
    pub value: i32,
    /// Timer flags for `TIMERS_UP`/`RACK_TIMES`.
    pub timer_flags: u8,
    /// Timer deadline for `TIMERS_UP`.
    pub timer_exp_us: u64,
    /// SACKed bytes for `RACK_TIMES`.
    pub sacked: u32,
    /// Holes retransmitted for `RACK_TIMES`.
    pub holes_rxt: u32,
    /// Considered-lost bytes for `RACK_TIMES`.
    pub considered_lost: u32,
}

/// The stack-info reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackInfo {
    /// Error code.
    pub error: i32,
    /// `t_sndbytes`.
    pub bytes_transmitted: u64,
    /// `t_snd_rxt_bytes`.
    pub bytes_retransmitted: u64,
}

/// The model's state for one installed function block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VtableModel {
    /// Registered name.
    pub name: &'static str,
    /// `tfb_flags`.
    pub flags: u32,
    /// `tfb_refcnt`.
    pub refcnt: u32,
    /// `t_fb_ptr` is allocated.
    pub fb_ptr: bool,
    /// `init` has run.
    pub initialized: bool,
    /// `r_ctl.fsb.hw_tls`.
    pub hw_tls: bool,
    /// `t_timers_stopped`.
    pub timers_stopped: bool,
    /// `rc_hpts_flags`.
    pub hpts_flags: u8,
    /// `t_in_hpts` is set.
    pub in_hpts: bool,
    /// `rc_last_output_to`.
    pub last_output_to_us: u64,
    /// `rc_timer_exp`.
    pub timer_exp_us: u64,
    /// Model clock.
    pub now_us: u64,
    /// `t_flags2`.
    pub flags2: u32,
    /// TCP state.
    pub state: TcpState,
    /// `TF_SENTFIN`.
    pub sent_fin: bool,
    /// `snd_una`.
    pub snd_una: u32,
    /// `snd_max`.
    pub snd_max: u32,
    /// `snd_nxt`.
    pub snd_nxt: u32,
    /// `snd_wnd`.
    pub snd_wnd: u32,
    /// `rc_sacked`.
    pub sacked: u32,
    /// `rc_considered_lost`.
    pub considered_lost: u32,
    /// `rc_holes_rxt`.
    pub holes_rxt: u32,
    /// `t_sndbytes`.
    pub bytes_transmitted: u64,
    /// `t_snd_rxt_bytes`.
    pub bytes_retransmitted: u64,
    /// `r_must_retran`.
    pub must_retran: bool,
    /// `t_rxtshift`.
    pub rxtshift: u8,
    /// The scoreboard was cleared by the retransmit timer.
    pub scoreboard_cleared: bool,
    /// Queued input segments.
    pub queued: u32,
    /// `t_inqueue` may accept mbufs.
    pub queue_ready: bool,
    /// The queued input saw a lost TCB.
    pub lost_tcb: bool,
    /// `t_rcvtime` for the last data segment.
    pub rcvtime_us: u64,
    /// Modelled options.
    pub options: RackOptions,
    /// Pacing is active.
    pub pacing: bool,
    /// SACK was negotiated.
    pub sack_permit: bool,
    /// `rack_sack_not_required`.
    pub sack_not_required: bool,
    /// `tcb_is_purged` from the last fini.
    pub purged: bool,
}

impl Default for VtableModel {
    fn default() -> Self {
        Self::new()
    }
}

impl VtableModel {
    /// Create an installed, not-yet-initialized function block.
    #[must_use]
    pub fn new() -> Self {
        Self {
            name: "rack",
            flags: RACK_FB_FLAGS,
            refcnt: 0,
            fb_ptr: false,
            initialized: false,
            hw_tls: false,
            timers_stopped: false,
            hpts_flags: 0,
            in_hpts: false,
            last_output_to_us: 0,
            timer_exp_us: 0,
            now_us: 0,
            flags2: 0,
            state: TcpState::Closed,
            sent_fin: false,
            snd_una: 0,
            snd_max: 0,
            snd_nxt: 0,
            snd_wnd: 0,
            sacked: 0,
            considered_lost: 0,
            holes_rxt: 0,
            bytes_transmitted: 0,
            bytes_retransmitted: 0,
            must_retran: false,
            rxtshift: 0,
            scoreboard_cleared: false,
            queued: 0,
            queue_ready: false,
            lost_tcb: false,
            rcvtime_us: 0,
            options: RackOptions::default(),
            pacing: true,
            sack_permit: false,
            sack_not_required: false,
            purged: false,
        }
    }

    /// Register the function block (`tfb_refcnt`).
    pub fn register(&mut self) -> i32 {
        let Some(next) = self.refcnt.checked_add(1) else {
            return EINVAL;
        };
        self.refcnt = next;
        0
    }

    /// Deregister the function block.
    pub fn deregister(&mut self) -> i32 {
        if self.refcnt == 0 {
            EINVAL
        } else {
            self.refcnt -= 1;
            0
        }
    }

    fn ensure(&self) -> Result<(), i32> {
        if self.fb_ptr { Ok(()) } else { Err(EINVAL) }
    }

    /// `tfb_tcp_fb_init`: allocate `t_fb_ptr`; double init is `EINVAL`.
    pub fn init(&mut self) -> i32 {
        if self.fb_ptr {
            return EINVAL;
        }
        self.fb_ptr = true;
        self.initialized = true;
        self.hw_tls = false;
        self.timers_stopped = false;
        self.hpts_flags = 0;
        self.in_hpts = false;
        self.timer_exp_us = 0;
        self.options = RackOptions::default();
        self.rxtshift = 0;
        self.must_retran = false;
        self.scoreboard_cleared = false;
        self.purged = false;
        0
    }

    /// `tfb_tcp_fb_fini`: release the per-connection state.
    pub fn fini(&mut self, purged: bool) {
        if self.fb_ptr {
            self.hw_tls = false;
            self.hpts_flags = 0;
            self.in_hpts = false;
            self.timer_exp_us = 0;
            self.timers_stopped = true;
            self.fb_ptr = false;
            self.initialized = false;
            self.purged = purged;
        }
    }

    /// `tfb_tcp_output`: retransmit, then send at `snd_max`.
    pub fn output(&mut self, data: u32) -> i32 {
        if let Err(error) = self.ensure() {
            return error;
        }
        let mut sent = false;
        if self.must_retran {
            let flight = self.snd_max.wrapping_sub(self.snd_una);
            self.bytes_retransmitted = self.bytes_retransmitted.saturating_add(u64::from(flight));
            self.must_retran = false;
            sent = true;
        }
        if data > 0 {
            self.snd_max = self.snd_max.wrapping_add(data);
            self.snd_nxt = self.snd_max;
            self.bytes_transmitted = self.bytes_transmitted.saturating_add(u64::from(data));
            sent = true;
        }
        if self.pacing && sent {
            self.hpts_flags |= PACE_PKT_OUTPUT;
            self.in_hpts = true;
            self.last_output_to_us = self.now_us.saturating_add(100);
        }
        0
    }

    /// `tfb_tcp_do_segment`: drain the queue, then process one segment.
    pub fn do_segment(&mut self, kind: SegmentKind, ack_bytes: u32) -> i32 {
        if let Err(error) = self.ensure() {
            return error;
        }
        self.queued = 0;
        match kind {
            SegmentKind::Rst => {
                self.state = TcpState::Closed;
                self.hpts_flags = 0;
                self.in_hpts = false;
            }
            SegmentKind::Ack => {
                if ack_bytes > 0 {
                    let next = self.snd_una.saturating_add(ack_bytes);
                    self.snd_una = next.min(self.snd_max);
                }
                self.hpts_flags &= !(PACE_TMR_MASK | PACE_PKT_OUTPUT);
                self.in_hpts = false;
                self.rxtshift = 0;
                self.must_retran = false;
            }
            SegmentKind::Data => {
                self.rcvtime_us = self.now_us;
            }
            SegmentKind::Fin => {
                self.state = TcpState::CloseWait;
            }
        }
        0
    }

    /// `tfb_do_segment_nounlock`: the unlocked input entry point.
    pub fn do_segment_nounlock(&mut self, kind: SegmentKind, ack_bytes: u32) -> i32 {
        self.do_segment(kind, ack_bytes)
    }

    /// `tfb_do_queued_segments`: drain; a lost TCB returns 1.
    pub fn do_queued_segments(&mut self, lost_tcb: bool) -> i32 {
        if let Err(error) = self.ensure() {
            return error;
        }
        self.queued = 0;
        self.lost_tcb = lost_tcb;
        if lost_tcb { 1 } else { 0 }
    }

    /// `tfb_tcp_ctloutput`: the modelled donor option subset.
    pub fn ctloutput(&mut self, dir: SockoptDir, option: RackOption, value: u32) -> CtlReply {
        if let Err(error) = self.ensure() {
            return CtlReply { error, value: None };
        }
        match dir {
            SockoptDir::Set => match option {
                RackOption::PaceMinSeg => {
                    self.options.pace_min_segs = u16::try_from(value & 0xffff).unwrap_or(u16::MAX);
                    CtlReply {
                        error: 0,
                        value: None,
                    }
                }
                RackOption::PaceMaxSeg => {
                    self.options.pace_max_segs = u16::try_from(value & 0xffff).unwrap_or(u16::MAX);
                    CtlReply {
                        error: 0,
                        value: None,
                    }
                }
                RackOption::PacingDivisor => {
                    self.options.pacing_divisor = if value == 0 {
                        DEFAULT_PACING_DIVISOR
                    } else {
                        value.max(RL_MIN_DIVISOR)
                    };
                    CtlReply {
                        error: 0,
                        value: None,
                    }
                }
                RackOption::PacingBeta => CtlReply {
                    error: EINVAL,
                    value: None,
                },
                RackOption::PacingBetaEcn => {
                    if self.options.newreno {
                        self.options.beta_ecn = value;
                        CtlReply {
                            error: 0,
                            value: None,
                        }
                    } else {
                        CtlReply {
                            error: EINVAL,
                            value: None,
                        }
                    }
                }
                RackOption::Unknown(_) => CtlReply {
                    error: EOPNOTSUPP,
                    value: None,
                },
            },
            SockoptDir::Get => match option {
                RackOption::PaceMinSeg => CtlReply {
                    error: 0,
                    value: Some(u32::from(self.options.pace_min_segs)),
                },
                RackOption::PaceMaxSeg => CtlReply {
                    error: 0,
                    value: Some(u32::from(self.options.pace_max_segs)),
                },
                RackOption::PacingDivisor => CtlReply {
                    error: 0,
                    value: Some(self.options.pacing_divisor),
                },
                RackOption::PacingBeta => {
                    if self.options.newreno {
                        CtlReply {
                            error: 0,
                            value: Some(self.options.beta),
                        }
                    } else {
                        CtlReply {
                            error: EINVAL,
                            value: None,
                        }
                    }
                }
                RackOption::PacingBetaEcn => {
                    if self.options.newreno {
                        CtlReply {
                            error: 0,
                            value: Some(self.options.beta_ecn),
                        }
                    } else {
                        CtlReply {
                            error: EINVAL,
                            value: None,
                        }
                    }
                }
                RackOption::Unknown(_) => CtlReply {
                    error: EOPNOTSUPP,
                    value: None,
                },
            },
        }
    }

    /// `tfb_tcp_timer_stop_all`.
    pub fn timer_stop_all(&mut self) -> i32 {
        if let Err(error) = self.ensure() {
            return error;
        }
        self.timers_stopped = true;
        self.hpts_flags = 0;
        self.in_hpts = false;
        self.timer_exp_us = 0;
        0
    }

    /// `tfb_tcp_rexmit_tmr`.
    pub fn rexmit_timer(&mut self) {
        if self.ensure().is_err() {
            return;
        }
        self.hpts_flags &= !PACE_TMR_RACK;
        self.in_hpts = false;
        self.must_retran = true;
        self.rxtshift = self.rxtshift.saturating_add(1);
        if self.rxtshift > RXT_SCOREBOARD_CLEAR_THRESH {
            self.scoreboard_cleared = true;
            self.sacked = 0;
            self.considered_lost = 0;
            self.holes_rxt = 0;
        }
    }

    /// `tfb_tcp_handoff_ok`.
    pub fn handoff_ok(&mut self) -> i32 {
        if let Err(error) = self.ensure() {
            return error;
        }
        match self.state {
            TcpState::Closed | TcpState::Listen => 0,
            TcpState::SynSent | TcpState::SynReceived => EAGAIN,
            _ => {
                if self.sent_fin && self.snd_max.wrapping_sub(self.snd_una) > 1 {
                    EAGAIN
                } else if self.sack_permit || self.sack_not_required {
                    0
                } else {
                    EINVAL
                }
            }
        }
    }

    /// `tfb_tcp_mtu_chg`.
    pub fn mtu_change(&mut self, mss: u16) {
        if self.ensure().is_err() {
            return;
        }
        if self.options.pace_min_segs != mss {
            self.options.pace_min_segs = mss;
            self.rexmit_timer();
            self.must_retran = true;
            self.snd_nxt = self.snd_max;
        }
    }

    /// `tfb_pru_options`: only `PRUS_OOB` is rejected.
    pub fn pru_options(&mut self, flags: u32) -> i32 {
        if let Err(error) = self.ensure() {
            return error;
        }
        if flags & PRUS_OOB != 0 { EOPNOTSUPP } else { 0 }
    }

    /// `tfb_hwtls_change`.
    pub fn hwtls_change(&mut self, enabled: bool) {
        if self.ensure().is_ok() {
            self.hw_tls = enabled;
        }
    }

    /// `tfb_chg_query`.
    pub fn chg_query(&mut self, query: Query) -> QueryReply {
        if let Err(error) = self.ensure() {
            return QueryReply {
                error,
                value: 0,
                timer_flags: 0,
                timer_exp_us: 0,
                sacked: 0,
                holes_rxt: 0,
                considered_lost: 0,
            };
        }
        match query {
            Query::SendMap => QueryReply {
                error: 0,
                value: 0,
                timer_flags: 0,
                timer_exp_us: 0,
                sacked: 0,
                holes_rxt: 0,
                considered_lost: 0,
            },
            Query::TimersUp => QueryReply {
                error: 0,
                value: if self.hpts_flags != 0 { 1 } else { 0 },
                timer_flags: self.hpts_flags,
                timer_exp_us: self.timer_exp_us,
                sacked: 0,
                holes_rxt: 0,
                considered_lost: 0,
            },
            Query::RackTimes => QueryReply {
                error: 0,
                value: 1,
                timer_flags: self.hpts_flags,
                timer_exp_us: self.timer_exp_us,
                sacked: self.sacked,
                holes_rxt: self.holes_rxt,
                considered_lost: self.considered_lost,
            },
            Query::Unknown(_) => QueryReply {
                error: -EINVAL,
                value: 0,
                timer_flags: 0,
                timer_exp_us: 0,
                sacked: 0,
                holes_rxt: 0,
                considered_lost: 0,
            },
        }
    }

    /// `tfb_switch_failed`: reapply flags and insert HPTS when not queued.
    pub fn switch_failed(&mut self) {
        if self.ensure().is_err() {
            return;
        }
        if self.queue_ready || self.pacing {
            self.flags2 |= TF2_SUPPORTS_MBUFQ;
        } else {
            self.flags2 &= !TF2_SUPPORTS_MBUFQ;
        }
        if self.queue_ready && self.state == TcpState::Established {
            self.flags2 |= TF2_MBUF_ACKCMP;
        }
        if self.in_hpts {
            return;
        }
        let delay = if self.hpts_flags & PACE_PKT_OUTPUT != 0 {
            self.last_output_to_us.saturating_sub(self.now_us)
        } else if self.hpts_flags & PACE_TMR_MASK != 0 {
            self.timer_exp_us.saturating_sub(self.now_us)
        } else {
            HPTS_USECS_PER_SLOT
        };
        self.hpts_flags |= PACE_TMR_RACK;
        self.in_hpts = true;
        self.timer_exp_us = self.now_us.saturating_add(delay.max(HPTS_USECS_PER_SLOT));
    }

    /// `tfb_early_wake_check`.
    pub fn early_wake_check(&self) -> bool {
        if !self.fb_ptr {
            return false;
        }
        if self.hpts_flags & PACE_PKT_OUTPUT != 0 {
            self.now_us >= self.last_output_to_us
        } else if self.hpts_flags & PACE_TMR_MASK != 0 {
            self.now_us >= self.timer_exp_us
        } else {
            false
        }
    }

    /// `tfb_compute_pipe`.
    pub fn compute_pipe(&self) -> i32 {
        if !self.fb_ptr {
            return -EINVAL;
        }
        let pipe = i64::from(self.snd_max)
            - i64::from(self.snd_una)
            - i64::from(self.sacked)
            - i64::from(self.considered_lost)
            + i64::from(self.holes_rxt);
        i32::try_from(pipe).unwrap_or(if pipe < 0 { i32::MIN } else { i32::MAX })
    }

    /// `tfb_stack_info`.
    pub fn stack_info(&self) -> StackInfo {
        if !self.fb_ptr {
            return StackInfo {
                error: EINVAL,
                bytes_transmitted: 0,
                bytes_retransmitted: 0,
            };
        }
        StackInfo {
            error: 0,
            bytes_transmitted: self.bytes_transmitted,
            bytes_retransmitted: self.bytes_retransmitted,
        }
    }

    /// `tfb_inherit`: copy the modelled options/flags tail only for the same stack.
    pub fn inherit(&mut self, source: &RackOptions, same_stack: bool) {
        if self.ensure().is_err() || !same_stack {
            return;
        }
        self.options = *source;
        self.flags2 = (self.flags2 & !(TF2_MBUF_ACKCMP | TF2_SUPPORTS_MBUFQ))
            | (source.flags2 & (TF2_MBUF_ACKCMP | TF2_SUPPORTS_MBUFQ));
    }

    /// Stable trace line shared with the K319/K320 consumers.
    #[must_use]
    pub fn trace_line(&self, seq: u32, callback: &str, args: &str, ret: i32) -> String {
        format!(
            "{seq}|{callback}|{args}|{ret}|{}|{}|{}|0x{:02x}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            u8::from(self.fb_ptr),
            u8::from(self.hw_tls),
            u8::from(self.timers_stopped),
            self.hpts_flags,
            u8::from(self.in_hpts),
            self.snd_una,
            self.snd_max,
            self.snd_nxt,
            self.sacked,
            self.considered_lost,
            self.holes_rxt,
            self.rxtshift,
            u8::from(self.must_retran)
        )
    }
}

/// One randomized callback event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// `tfb_tcp_fb_init`.
    Init,
    /// `tfb_tcp_fb_fini`.
    Fini(bool),
    /// `tfb_tcp_output(data)`.
    Output(u32),
    /// `tfb_tcp_do_segment`.
    Segment(SegmentKind, u32),
    /// `tfb_do_segment_nounlock`.
    SegmentUnlock(SegmentKind, u32),
    /// `tfb_do_queued_segments`.
    Queued(bool),
    /// `tfb_tcp_ctloutput`.
    Ctl(SockoptDir, RackOption, u32),
    /// `tfb_tcp_timer_stop_all`.
    TimerStop,
    /// `tfb_tcp_rexmit_tmr`.
    Rexmit,
    /// `tfb_tcp_handoff_ok`.
    Handoff,
    /// `tfb_tcp_mtu_chg`.
    Mtu(u16),
    /// `tfb_pru_options`.
    Pru(u32),
    /// `tfb_hwtls_change`.
    Hwtls(bool),
    /// `tfb_chg_query`.
    Query(Query),
    /// `tfb_switch_failed`.
    Switch,
    /// `tfb_early_wake_check`.
    Wake,
    /// `tfb_compute_pipe`.
    Pipe,
    /// `tfb_stack_info`.
    Info,
    /// `tfb_inherit`.
    Inherit(bool),
}

/// A callback result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventResult {
    /// Integer return.
    Int(i32),
    /// Boolean return.
    Bool(bool),
    /// Control reply.
    Ctl(CtlReply),
    /// Query reply.
    Query(QueryReply),
    /// Stack info.
    Info(StackInfo),
    /// Void callback.
    None,
}

impl EventResult {
    /// The integer value used in the trace line.
    #[must_use]
    pub fn as_i32(self) -> i32 {
        match self {
            Self::Int(value) => value,
            Self::Bool(value) => {
                if value {
                    1
                } else {
                    0
                }
            }
            Self::Ctl(reply) => reply.error,
            Self::Query(reply) => {
                if reply.error != 0 {
                    reply.error
                } else {
                    reply.value
                }
            }
            Self::Info(info) => info.error,
            Self::None => 0,
        }
    }
}

impl Event {
    /// Callback name for the trace.
    #[must_use]
    pub const fn callback(self) -> &'static str {
        match self {
            Self::Init => "tfb_tcp_fb_init",
            Self::Fini(_) => "tfb_tcp_fb_fini",
            Self::Output(_) => "tfb_tcp_output",
            Self::Segment(..) => "tfb_tcp_do_segment",
            Self::SegmentUnlock(..) => "tfb_do_segment_nounlock",
            Self::Queued(_) => "tfb_do_queued_segments",
            Self::Ctl(..) => "tfb_tcp_ctloutput",
            Self::TimerStop => "tfb_tcp_timer_stop_all",
            Self::Rexmit => "tfb_tcp_rexmit_tmr",
            Self::Handoff => "tfb_tcp_handoff_ok",
            Self::Mtu(_) => "tfb_tcp_mtu_chg",
            Self::Pru(_) => "tfb_pru_options",
            Self::Hwtls(_) => "tfb_hwtls_change",
            Self::Query(_) => "tfb_chg_query",
            Self::Switch => "tfb_switch_failed",
            Self::Wake => "tfb_early_wake_check",
            Self::Pipe => "tfb_compute_pipe",
            Self::Info => "tfb_stack_info",
            Self::Inherit(_) => "tfb_inherit",
        }
    }

    /// Stable argument string for the trace.
    #[must_use]
    pub fn args(self) -> String {
        match self {
            Self::Init
            | Self::TimerStop
            | Self::Rexmit
            | Self::Handoff
            | Self::Switch
            | Self::Wake
            | Self::Pipe
            | Self::Info => String::new(),
            Self::Fini(purged) => format!("purged={purged}"),
            Self::Output(data) => format!("data={data}"),
            Self::Segment(kind, ack) | Self::SegmentUnlock(kind, ack) => {
                format!("kind={kind:?},ack={ack}")
            }
            Self::Queued(lost) => format!("lost_tcb={lost}"),
            Self::Ctl(dir, option, value) => format!("dir={dir:?},opt={option:?},value={value}"),
            Self::Mtu(mss) => format!("mss={mss}"),
            Self::Pru(flags) => format!("flags={flags}"),
            Self::Hwtls(enabled) => format!("enabled={enabled}"),
            Self::Query(query) => format!("query={query:?}"),
            Self::Inherit(same) => format!("same_stack={same}"),
        }
    }

    /// Apply the event and return the callback result.
    pub fn apply(self, model: &mut VtableModel) -> EventResult {
        model.now_us = model.now_us.saturating_add(1_000);
        match self {
            Self::Init => EventResult::Int(model.init()),
            Self::Fini(purged) => {
                model.fini(purged);
                EventResult::None
            }
            Self::Output(data) => EventResult::Int(model.output(data)),
            Self::Segment(kind, ack) => EventResult::Int(model.do_segment(kind, ack)),
            Self::SegmentUnlock(kind, ack) => {
                EventResult::Int(model.do_segment_nounlock(kind, ack))
            }
            Self::Queued(lost) => EventResult::Int(model.do_queued_segments(lost)),
            Self::Ctl(dir, option, value) => EventResult::Ctl(model.ctloutput(dir, option, value)),
            Self::TimerStop => EventResult::Int(model.timer_stop_all()),
            Self::Rexmit => {
                model.rexmit_timer();
                EventResult::None
            }
            Self::Handoff => EventResult::Int(model.handoff_ok()),
            Self::Mtu(mss) => {
                model.mtu_change(mss);
                EventResult::None
            }
            Self::Pru(flags) => EventResult::Int(model.pru_options(flags)),
            Self::Hwtls(enabled) => {
                model.hwtls_change(enabled);
                EventResult::None
            }
            Self::Query(query) => EventResult::Query(model.chg_query(query)),
            Self::Switch => {
                model.switch_failed();
                EventResult::None
            }
            Self::Wake => EventResult::Bool(model.early_wake_check()),
            Self::Pipe => EventResult::Int(model.compute_pipe()),
            Self::Info => EventResult::Info(model.stack_info()),
            Self::Inherit(same) => {
                let source = RackOptions {
                    pace_min_segs: 1_200,
                    pace_max_segs: 1_460,
                    pacing_divisor: 100,
                    beta: 60,
                    beta_ecn: 70,
                    newreno: true,
                    flags2: TF2_SUPPORTS_MBUFQ,
                };
                model.inherit(&source, same);
                EventResult::None
            }
        }
    }
}
