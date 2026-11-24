// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Congestion-control framework: NewReno, CUBIC and PRR in integer arithmetic.
//!
//! NewReno is adapted from FreeBSD `sys/netinet/cc/cc_newreno.c` and `sys/netinet/cc/cc.c`
//! (BSD-2-Clause; Copyright (c) 2007-2008, 2010, 2014 Swinburne University of Technology,
//! Copyright (c) 2009-2010 Lawrence Stewart, Copyright (c) 2010 The FreeBSD Foundation).
//! CUBIC is adapted from FreeBSD `sys/netinet/cc/cc_cubic.c` and `cc_cubic.h` (BSD-2-Clause;
//! Copyright (c) 2008-2010 Lawrence Stewart, Copyright (c) 2010 The FreeBSD Foundation).
//! The Rust expression is first-party.
//!
//! K258 also read NetBSD `sys/netinet/tcp_congctl.c` (4-clause NRL: read-only, no code copied).
//! Its two-MSS partial-ACK floor and outstanding-data recovery exit are retained where they are
//! compatible with the FreeBSD/K269 controller; the FreeBSD ABC congestion-avoidance boundary and
//! first-RTO cut are authoritative and unchanged. CUBIC and RACK remain first-party extensions.

use crate::Duration;

/// The reason a loss was reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LossKind {
    /// Duplicate-ACK threshold.
    DupAck,
    /// SACK loss detection.
    Sack,
    /// Explicit congestion notification.
    Ecn,
}

/// A read-only view passed to a congestion algorithm.
#[derive(Clone, Copy, Debug, Default)]
pub struct CcContext {
    /// Sender MSS in bytes.
    pub mss: u32,
    /// Bytes in flight.
    pub flight: u32,
    /// RFC 6675 Pipe estimate.
    pub pipe: u32,
    /// Current smoothed RTT.
    pub rtt: Option<Duration>,
    /// Highest sequence sent.
    pub snd_nxt: u32,
    /// Oldest unacknowledged sequence.
    pub snd_una: u32,
}

/// An explicit CUBIC state-machine input with injected time and RTT.
///
/// This is the FreeBSD `cc_var`/`tcpcb` subset used by the ported `cc_cubic.c` entry points. It is
/// a sans-IO type: the caller injects the clock, the send window and the ACK state, and owns the
/// transport wiring. A zero `cwnd`, `mss` or `rtt_updated` means "use the controller's own value".
#[derive(Clone, Copy, Debug, Default)]
pub struct CubicContext {
    /// Monotonic time in microseconds.
    pub now_us: i64,
    /// Smoothed RTT in milliseconds.
    pub srtt_ms: u32,
    /// Current MSS.
    pub mss: u32,
    /// Receiver send window.
    pub snd_wnd: u32,
    /// Congestion window at entry.
    pub cwnd: u32,
    /// RFC 6675 pipe estimate.
    pub pipe: u32,
    /// Bytes acknowledged by this event (`bytes_this_ack`).
    pub acked: u32,
    /// Segments acknowledged by this event.
    pub nsegs: u32,
    /// Highest sequence sent.
    pub snd_nxt: u32,
    /// Highest sequence ever sent.
    pub snd_max: u32,
    /// CWND-limited flag (`CCF_CWND_LIMITED`).
    pub cwnd_limited: bool,
    /// Number of RTT samples taken (`t_rttupdated`).
    pub rtt_updated: u32,
    /// Consecutive RTO count (`t_rxtshift`).
    pub rxtshift: u32,
    /// Hystart++ is allowed on this connection (`CCF_HYSTART_ALLOWED`).
    pub hystart_allowed: bool,
}

/// A FreeBSD `ccsignal_t` congestion signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CubicSignal {
    /// `CC_NDUPACK`: duplicate-ACK or SACK loss.
    DupAck,
    /// `CC_ECN`: ECN congestion mark.
    Ecn,
    /// `CC_RTO`: retransmission timeout.
    Rto,
    /// `CC_RTO_ERR`: the RTO was spurious.
    RtoErr,
}

/// A pluggable congestion-control algorithm.
pub trait CongestionControl {
    /// Algorithm name for observability.
    fn name(&self) -> &'static str;
    /// A new-data ACK.
    fn on_ack(&mut self, ctx: &mut CcContext, acked_bytes: u32, rtt: Option<Duration>);
    /// A loss indication.
    fn on_loss(&mut self, ctx: &mut CcContext, kind: LossKind);
    /// An RTO.
    fn on_rto(&mut self, ctx: &mut CcContext);
    /// Current congestion window.
    fn cwnd(&self) -> u32;
    /// Current slow-start threshold.
    fn ssthresh(&self) -> u32;
    /// Whether fast recovery is active.
    fn in_recovery(&self) -> bool;
    /// A partial ACK inside recovery (RFC 6582).
    fn on_partial_ack(&mut self, _acked_bytes: u32) {}
    /// The recovery episode is complete; return to congestion avoidance.
    fn on_recovery_exit(&mut self) {}
    /// The loss response was spurious: restore the window state from before it.
    fn undo(&mut self, _cwnd: u32, _ssthresh: u32) {}
}

/// NewReno with FreeBSD `cc_newreno.c`/`cc.c` ABC and recovery semantics.
///
/// The loss/recovery arithmetic follows the FreeBSD `newreno_cc_cong_signal`,
/// `newreno_cc_cwnd_in_cong_avoid`, `newreno_cc_cwnd_in_slow_start`,
/// `newreno_cc_after_idle`, `newreno_cc_post_recovery` and `tcp_newreno_partial_ack` behaviour.
/// The two-MSS partial-ACK floor and the outstanding-data recovery exit follow the NetBSD
/// `tcp_congctl.c` reading where they do not change a K269 vector row. CUBIC and RACK remain
/// first-party extensions.
#[derive(Clone, Debug)]
pub struct NewReno {
    cwnd: u32,
    ssthresh: u32,
    mss: u32,
    cwnd_max: u32,
    dup_threshold: u32,
    recover: u32,
    last_ack: Option<u32>,
    bytes_acked: u32,
    in_recovery: bool,
    in_rto: bool,
}

impl NewReno {
    /// Construct a NewReno controller.
    #[must_use]
    pub fn new(mss: u32, initial_cwnd: u32, initial_ssthresh: u32, cwnd_max: u32) -> Self {
        Self {
            cwnd: initial_cwnd.max(2 * mss),
            ssthresh: initial_ssthresh,
            mss: mss.max(1),
            cwnd_max: cwnd_max.max(2 * mss),
            dup_threshold: 3,
            recover: 0,
            last_ack: None,
            bytes_acked: 0,
            in_recovery: false,
            in_rto: false,
        }
    }

    /// Set the duplicate-ACK threshold.
    pub fn set_dup_threshold(&mut self, threshold: u32) {
        self.dup_threshold = threshold.max(1);
    }

    /// Apply a fast-retransmit loss event with the flight size at the event.
    pub fn loss_fast_retransmit(&mut self, flight: u32, snd_nxt: u32) {
        // NetBSD uses min(snd_wnd, snd_cwnd) in tcp_common_congestion_exp. The CcContext has no
        // snd_wnd field, and `flight` is bounded by the receiver window and cwnd at the loss
        // point, so it is the closest available proxy.
        let win = flight
            .min(self.cwnd)
            .checked_div(2)
            .unwrap_or(0)
            .checked_div(self.mss)
            .unwrap_or(0)
            .max(2);
        self.ssthresh = win.saturating_mul(self.mss);
        self.cwnd = self
            .ssthresh
            .saturating_add(self.dup_threshold.saturating_mul(self.mss));
        self.recover = snd_nxt;
        self.last_ack = None;
        self.bytes_acked = 0;
        self.in_recovery = true;
    }

    /// Apply a partial ACK inside recovery.
    pub fn partial_ack(&mut self, acked: u32) {
        if !self.in_recovery {
            return;
        }
        // NetBSD tcp_newreno_fast_retransmit_newack: deflate by (acked - MSS).
        self.cwnd = self
            .cwnd
            .saturating_add(self.mss)
            .saturating_sub(acked)
            .max(2 * self.mss);
    }

    /// Apply a full ACK at or above `recover`.
    pub fn full_ack(&mut self) {
        if self.in_recovery {
            if let Some(ack) = self.last_ack {
                let outstanding = self.recover.saturating_sub(ack);
                self.cwnd = if outstanding < self.ssthresh {
                    outstanding.saturating_add(self.mss)
                } else {
                    self.ssthresh
                };
            } else {
                self.cwnd = self.ssthresh;
            }
            self.bytes_acked = 0;
            self.last_ack = None;
            self.in_recovery = false;
        }
    }

    /// Idle restart: FreeBSD `newreno_cc_after_idle`.
    pub fn idle_restart(&mut self) {
        self.ssthresh = self.ssthresh.max(self.cwnd.saturating_sub(self.cwnd >> 2));
        self.cwnd = self.cwnd.min(10 * self.mss);
    }
}

impl CongestionControl for NewReno {
    fn name(&self) -> &'static str {
        "newreno"
    }

    fn on_ack(&mut self, ctx: &mut CcContext, acked_bytes: u32, _rtt: Option<Duration>) {
        self.last_ack = Some(ctx.snd_una.saturating_add(acked_bytes));
        if self.in_recovery {
            return;
        }
        if self.in_rto {
            self.in_rto = false;
        }
        if self.cwnd > self.ssthresh {
            // FreeBSD ABC: count acked bytes up to abc_l_var*MSS.
            let abc = acked_bytes.min(self.mss.saturating_mul(2));
            self.bytes_acked = self.bytes_acked.saturating_add(abc);
            if self.bytes_acked >= self.cwnd {
                self.bytes_acked = self.bytes_acked.saturating_sub(self.cwnd);
                self.cwnd = self.cwnd.saturating_add(self.mss);
            }
        } else {
            // FreeBSD slow start with abc_l_var=2 while data is still queued.
            self.cwnd = self
                .cwnd
                .saturating_add(acked_bytes.min(self.mss.saturating_mul(2)));
        }
        self.cwnd = self.cwnd.min(self.cwnd_max);
    }

    fn on_loss(&mut self, ctx: &mut CcContext, kind: LossKind) {
        if kind == LossKind::Ecn {
            self.ssthresh = self
                .cwnd
                .saturating_mul(50)
                .checked_div(100)
                .unwrap_or(0)
                .checked_div(self.mss)
                .unwrap_or(0)
                .max(2)
                .saturating_mul(self.mss);
            self.cwnd = self.ssthresh;
            self.in_recovery = true;
            self.bytes_acked = 0;
            self.last_ack = None;
        } else {
            self.loss_fast_retransmit(ctx.flight, ctx.snd_nxt);
        }
    }

    fn on_rto(&mut self, ctx: &mut CcContext) {
        // FreeBSD newreno_cc_cong_signal: only the first RTO (t_rxtshift==1) re-cuts.
        if !self.in_rto {
            let win = ctx
                .flight
                .min(self.cwnd)
                .checked_div(2)
                .unwrap_or(0)
                .checked_div(self.mss)
                .unwrap_or(0)
                .max(2);
            self.ssthresh = win.saturating_mul(self.mss);
            self.in_rto = true;
        }
        self.cwnd = self.mss;
        self.bytes_acked = 0;
        self.last_ack = None;
        self.in_recovery = false;
        self.in_rto = true;
    }

    fn cwnd(&self) -> u32 {
        self.cwnd
    }
    fn ssthresh(&self) -> u32 {
        self.ssthresh
    }
    fn in_recovery(&self) -> bool {
        self.in_recovery
    }
    fn on_partial_ack(&mut self, acked_bytes: u32) {
        self.partial_ack(acked_bytes);
    }
    fn on_recovery_exit(&mut self) {
        self.full_ack();
    }
    fn undo(&mut self, cwnd: u32, ssthresh: u32) {
        self.cwnd = cwnd.max(2 * self.mss);
        self.ssthresh = ssthresh;
        self.in_recovery = false;
    }
}

/// FreeBSD CUBIC, a literal port of `sys/netinet/cc/cc_cubic.c` (BSD-2-Clause).
///
/// The fields mirror `struct cubic` from `cc_cubic.h`, including the Hystart++ CSS state. The
/// public methods are the donor entry points: [`Cubic::ack_received`], [`Cubic::congestion`],
/// [`Cubic::post_recovery`], [`Cubic::record_rtt`], [`Cubic::get_ssthresh`],
/// [`Cubic::after_idle`], [`Cubic::newround`] and [`Cubic::rttsample`]. Every input is injected
/// through [`CubicContext`]; the type never reads a clock or the transport.
#[derive(Clone, Debug)]
pub struct Cubic {
    cwnd: u32,
    ssthresh: u32,
    mss: u32,
    cwnd_max: u32,
    in_recovery: bool,
    /// `struct cubic.K`, seconds in `CUBIC_SHIFT` fixed point.
    k: i64,
    /// `struct cubic.sum_rtt_usecs`.
    sum_rtt_us: i64,
    /// `struct cubic.W_max`.
    w_max: u32,
    /// `struct cubic.cwnd_epoch`.
    cwnd_epoch: u32,
    /// `struct cubic.flags`.
    flags: u32,
    /// `struct cubic.min_rtt_usecs`.
    min_rtt_us: i32,
    /// `struct cubic.mean_rtt_usecs`.
    mean_rtt_us: i32,
    /// `struct cubic.epoch_ack_count`.
    epoch_ack_count: i32,
    /// `struct cubic.t_epoch`, microseconds.
    t_epoch: i64,
    undo_t_epoch: i64,
    undo_k: i64,
    undo_w_max: u32,
    undo_cwnd_epoch: u32,
    css_baseline_minrtt: u32,
    css_current_round_minrtt: u32,
    css_lastround_minrtt: u32,
    css_rttsample_count: u32,
    css_entered_at_round: u32,
    css_current_round: u32,
    css_fas_at_css_entry: u32,
    css_lowrtt_fas: u32,
    css_last_fas: u32,
    /// Transport-visible `t_rttupdated` for the port.
    rtt_updated: u32,
    /// Transport-visible `t_rxtshift` for the port.
    rxtshift: u32,
    /// ABC bytes accumulated for the `CCF_ABC_SENTAWND` equivalent.
    abc_bytes: u32,
    /// Last pipe estimate seen, used by the live recovery-exit adapter.
    last_pipe: u32,
    /// Logical clock advanced by the live `CcContext` adapter.
    clock_us: i64,
}

/// Number of fixed-point fraction bits in `cubic_k` and `cubic_cwnd`.
const CUBIC_SHIFT: u32 = 8;
/// Bits shifted out after multiplying by `CUBIC_C_FACTOR` (four times `CUBIC_SHIFT`).
const CUBIC_SHIFT_4: u32 = 32;
/// ~0.7 in `CUBIC_SHIFT` fixed point.
const CUBIC_BETA: u64 = 179;
/// `(1 + beta) / 2` in `CUBIC_SHIFT` fixed point, for fast convergence.
const CUBIC_FC: u64 = 217;
/// ~0.4 in `CUBIC_SHIFT` fixed point.
const CUBIC_C_FACTOR: u64 = 102;
/// Effective cube-root limit from `cc_cubic.h`.
const CUBED_ROOT_MAX: i64 = 448_845;
/// Do not trust SRTT until this many samples.
const CUBIC_MIN_RTT_SAMPLES: u32 = 8;
/// `V_tcp_abc_l_var` default.
const CUBIC_ABC_L_VAR: u32 = 2;
/// `V_tcp_initcwnd_segments` default; also the idle restart window.
const CUBIC_INITCWND_SEGMENTS: u32 = 10;
/// Default `V_tcp_do_rfc3465`.
const CUBIC_DO_RFC3465: bool = true;
/// `V_newreno_beta`, percent.
const CUBIC_NEWRENO_BETA: u64 = 50;

const CUBICFLAG_CONG_EVENT: u32 = 0x0000_0001;
const CUBICFLAG_IN_SLOWSTART: u32 = 0x0000_0002;
const CUBICFLAG_IN_APPLIMIT: u32 = 0x0000_0004;
const CUBICFLAG_RTO_EVENT: u32 = 0x0000_0008;
const CUBICFLAG_HYSTART_ENABLED: u32 = 0x0000_0010;
const CUBICFLAG_HYSTART_IN_CSS: u32 = 0x0000_0020;
const CUBICFLAG_IN_TF: u32 = 0x0000_0040;

/// `hystart_n_rttsamples` default.
const HYSTART_N_RTTSAMPLES: u32 = 8;
/// `hystart_minrtt_thresh` default, microseconds.
const HYSTART_MINRTT_THRESH_US: u32 = 4_000;
/// `hystart_maxrtt_thresh` default, microseconds.
const HYSTART_MAXRTT_THRESH_US: u32 = 16_000;
/// `hystart_css_growth_div` default.
const HYSTART_CSS_GROWTH_DIV: u32 = 4;
/// `hystart_css_rounds` default.
const HYSTART_CSS_ROUNDS: u32 = 5;
/// The CSS "infinity" sentinel (`0xffffffff`).
const CSS_INFINITY: u32 = 0xffff_ffff;

impl Cubic {
    /// Construct a CUBIC controller.
    #[must_use]
    pub fn new(mss: u32, initial_cwnd: u32, initial_ssthresh: u32, cwnd_max: u32) -> Self {
        let mss = mss.max(1);
        Self {
            cwnd: initial_cwnd.max(2 * mss),
            ssthresh: initial_ssthresh,
            mss,
            cwnd_max: cwnd_max.max(2 * mss),
            in_recovery: false,
            k: 0,
            sum_rtt_us: 0,
            // cubic_conn_init: avoid hostcache entries starting with a stale W_max.
            w_max: u32::MAX,
            cwnd_epoch: 0,
            // cubic_cb_init: Hystart++ starts enabled.
            flags: CUBICFLAG_HYSTART_ENABLED,
            min_rtt_us: 0,
            mean_rtt_us: 1,
            epoch_ack_count: 0,
            t_epoch: 0,
            undo_t_epoch: 0,
            undo_k: 0,
            undo_w_max: 0,
            undo_cwnd_epoch: 0,
            css_baseline_minrtt: CSS_INFINITY,
            css_current_round_minrtt: CSS_INFINITY,
            css_lastround_minrtt: CSS_INFINITY,
            css_rttsample_count: 0,
            css_entered_at_round: 0,
            css_current_round: 0,
            css_fas_at_css_entry: 0,
            css_lowrtt_fas: 0,
            css_last_fas: 0,
            rtt_updated: 0,
            rxtshift: 0,
            abc_bytes: 0,
            last_pipe: 0,
            clock_us: 0,
        }
    }

    /// The current `W_max` in bytes.
    #[must_use]
    pub const fn w_max(&self) -> u32 {
        self.w_max
    }

    /// The current `K`, in `CUBIC_SHIFT` fixed-point seconds.
    #[must_use]
    pub const fn k(&self) -> i64 {
        self.k
    }

    /// The current `K` in microseconds.
    #[must_use]
    pub fn k_us(&self) -> u128 {
        if self.k <= 0 {
            0
        } else {
            (self.k as u128).saturating_mul(1_000_000) >> CUBIC_SHIFT
        }
    }

    /// The minimum observed RTT in microseconds.
    #[must_use]
    pub const fn min_rtt_us(&self) -> i32 {
        self.min_rtt_us
    }

    /// The mean observed RTT between epochs, in microseconds.
    #[must_use]
    pub const fn mean_rtt_us(&self) -> i32 {
        self.mean_rtt_us
    }

    /// The CUBIC and Hystart++ flags (`struct cubic.flags`).
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    fn effective_mss(&mut self, ctx: &CubicContext) -> u32 {
        if ctx.mss != 0 {
            self.mss = ctx.mss.max(1);
        }
        self.mss
    }

    /// `cubic_k` from `cc_cubic.h` (Apple TR KT-32).
    fn cubic_k(wmax_segs: u32, cwnd_epoch_segs: u32) -> i64 {
        if wmax_segs <= cwnd_epoch_segs {
            return 0;
        }
        let mut s =
            (i64::from(wmax_segs - cwnd_epoch_segs) << (2 * CUBIC_SHIFT)) / CUBIC_C_FACTOR as i64;
        let mut p = 0u32;
        while s >= 256 {
            s >>= 3;
            p = p.saturating_add(1);
        }
        let mut k =
            (((s * 275) >> CUBIC_SHIFT) + 98) - (((s * s * 120) >> CUBIC_SHIFT) >> CUBIC_SHIFT);
        k <<= p;
        k
    }

    /// `cubic_cwnd` from `cc_cubic.h`.
    fn cubic_cwnd(usecs_since_epoch: i64, wmax: u32, smss: u32, k: i64) -> u32 {
        let numerator = (i128::from(usecs_since_epoch) << CUBIC_SHIFT)
            - i128::from(k).saturating_mul(1_000_000);
        let delta = numerator / 1_000_000;
        if delta > i128::from(CUBED_ROOT_MAX) {
            return i32::MAX as u32;
        }
        if delta < -i128::from(CUBED_ROOT_MAX) {
            return 0;
        }
        let cube = delta.saturating_mul(delta).saturating_mul(delta);
        let term = (cube.saturating_mul(i128::from(CUBIC_C_FACTOR))) >> CUBIC_SHIFT_4;
        let value = term
            .saturating_mul(i128::from(smss))
            .saturating_add(i128::from(wmax))
            .max(0);
        u32::try_from(value).unwrap_or(u32::MAX)
    }

    /// `newreno_cc_cwnd_in_cong_avoid` with the default RFC 3465 ABC path.
    fn tf_cwnd(cwnd: u32, mss: u32, abc_sent_awnd: bool, cwnd_max: u32) -> u32 {
        let incr = if CUBIC_DO_RFC3465 {
            if abc_sent_awnd { mss } else { 0 }
        } else {
            (mss.saturating_mul(mss) / cwnd.max(1)).max(1)
        };
        if incr > 0 {
            cwnd.saturating_add(incr).min(cwnd_max)
        } else {
            cwnd
        }
    }

    /// `cubic_does_slow_start`: ABC slow start, plus the Hystart++ CSS entry check.
    fn does_slow_start(&mut self, ctx: &CubicContext) {
        let mss = self.effective_mss(ctx);
        self.flags |= CUBICFLAG_IN_SLOWSTART;
        if ctx.hystart_allowed
            && self.flags & CUBICFLAG_HYSTART_ENABLED != 0
            && self.flags & CUBICFLAG_HYSTART_IN_CSS == 0
            && self.css_rttsample_count >= HYSTART_N_RTTSAMPLES
            && self.css_current_round_minrtt != CSS_INFINITY
            && self.css_lastround_minrtt != CSS_INFINITY
        {
            let mut rtt_thresh = self.css_lastround_minrtt >> 3;
            rtt_thresh = rtt_thresh.max(HYSTART_MINRTT_THRESH_US);
            rtt_thresh = rtt_thresh.min(HYSTART_MAXRTT_THRESH_US);
            if self.css_current_round_minrtt >= self.css_lastround_minrtt.saturating_add(rtt_thresh)
            {
                self.flags |= CUBICFLAG_HYSTART_IN_CSS;
                self.css_fas_at_css_entry = self.css_lowrtt_fas;
                self.css_baseline_minrtt = self.css_current_round_minrtt;
                self.css_entered_at_round = self.css_current_round;
            }
        }
        let mut incr = if ctx.snd_nxt == ctx.snd_max {
            ctx.acked.min(
                ctx.nsegs
                    .max(1)
                    .saturating_mul(CUBIC_ABC_L_VAR)
                    .saturating_mul(mss),
            )
        } else {
            ctx.acked.min(mss)
        };
        if self.flags & CUBICFLAG_HYSTART_IN_CSS != 0 {
            incr /= HYSTART_CSS_GROWTH_DIV;
        }
        if incr > 0 {
            self.cwnd = self.cwnd.saturating_add(incr).min(self.cwnd_max);
        }
    }

    /// `cubic_record_rtt`: track minimum RTT and the epoch sample sum.
    pub fn record_rtt(&mut self, ctx: &CubicContext) {
        let rtt_updated = if ctx.rtt_updated != 0 {
            ctx.rtt_updated
        } else {
            self.rtt_updated
        };
        if rtt_updated < CUBIC_MIN_RTT_SAMPLES {
            return;
        }
        let t_srtt_us = ctx.srtt_ms.saturating_mul(1_000).max(1);
        if self.min_rtt_us == 0 || i64::from(t_srtt_us) < i64::from(self.min_rtt_us) {
            self.min_rtt_us = i32::try_from(t_srtt_us).unwrap_or(i32::MAX);
            if self.min_rtt_us > self.mean_rtt_us {
                self.mean_rtt_us = self.min_rtt_us;
            }
        }
        self.sum_rtt_us = self.sum_rtt_us.saturating_add(i64::from(t_srtt_us));
        self.epoch_ack_count = self.epoch_ack_count.saturating_add(1);
    }

    /// `cubic_ack_received`: one ACK through slow start or the CUBIC curve.
    pub fn ack_received(&mut self, ctx: &CubicContext) {
        let mss = self.effective_mss(ctx);
        let cwin = if ctx.cwnd == 0 { self.cwnd } else { ctx.cwnd };
        self.cwnd = cwin;
        self.last_pipe = ctx.pipe;
        self.record_rtt(ctx);

        if self.in_recovery {
            return;
        }
        if !ctx.cwnd_limited {
            self.flags |= CUBICFLAG_IN_APPLIMIT;
            return;
        }
        if cwin <= self.ssthresh || self.min_rtt_us == 0 {
            self.does_slow_start(ctx);
            return;
        }

        if self.flags & CUBICFLAG_HYSTART_IN_CSS != 0 {
            self.flags &= !CUBICFLAG_HYSTART_IN_CSS;
            self.flags &= !CUBICFLAG_HYSTART_ENABLED;
        }
        if self.flags & (CUBICFLAG_IN_SLOWSTART | CUBICFLAG_CONG_EVENT | CUBICFLAG_IN_APPLIMIT) != 0
        {
            self.t_epoch = ctx.now_us;
            self.cwnd_epoch = cwin;
            self.k = Self::cubic_k(self.w_max / mss, self.cwnd_epoch / mss);
            self.flags &= !(CUBICFLAG_IN_SLOWSTART | CUBICFLAG_CONG_EVENT | CUBICFLAG_IN_APPLIMIT);
            if self.flags & CUBICFLAG_RTO_EVENT != 0 {
                self.flags &= !CUBICFLAG_RTO_EVENT;
                self.w_max = cwin;
                self.k = 0;
            }
        }
        let mut usecs_since_epoch = ctx.now_us.saturating_sub(self.t_epoch);
        if usecs_since_epoch < 0 {
            usecs_since_epoch = i64::from(i32::MAX);
            self.t_epoch = ctx.now_us.saturating_sub(i64::from(i32::MAX));
        }

        let abc = self
            .abc_bytes
            .saturating_add(ctx.acked.min(mss.saturating_mul(CUBIC_ABC_L_VAR)));
        let abc_sent_awnd = abc >= cwin;
        self.abc_bytes = if abc_sent_awnd { abc - cwin } else { abc };
        let w_est = Self::tf_cwnd(cwin, mss, abc_sent_awnd, self.cwnd_max);
        let w_cubic = Self::cubic_cwnd(
            usecs_since_epoch.saturating_add(i64::from(self.mean_rtt_us)),
            self.w_max,
            mss,
            self.k,
        );
        if w_cubic < w_est {
            self.cwnd = w_est;
            self.flags |= CUBICFLAG_IN_TF;
        } else {
            let target = if w_cubic < cwin {
                cwin
            } else if w_cubic > cwin.saturating_mul(3) >> 1 {
                cwin.saturating_mul(3) >> 1
            } else {
                w_cubic
            };
            let incr = ((i64::from(target.saturating_sub(cwin)) << CUBIC_SHIFT)
                / i64::from(cwin.max(1))
                * i64::from(mss))
                >> CUBIC_SHIFT;
            self.cwnd = cwin.saturating_add(u32::try_from(incr.max(0)).unwrap_or(u32::MAX));
        }
    }

    /// `cubic_get_ssthresh`: fast convergence plus the multiplicative decrease.
    pub fn get_ssthresh(&mut self, ctx: &CubicContext) -> u32 {
        let mss = self.effective_mss(ctx);
        let mut cwnd = self.cwnd;
        if cwnd < self.w_max {
            cwnd = u32::try_from((u64::from(cwnd) * CUBIC_FC) >> CUBIC_SHIFT).unwrap_or(cwnd);
        }
        self.w_max = cwnd;
        if self.flags & CUBICFLAG_IN_TF != 0 {
            let factor = CUBIC_NEWRENO_BETA;
            let segments = (u64::from(cwnd) * factor) / (100 * u64::from(mss.max(1)));
            u32::try_from(segments.max(2))
                .unwrap_or(u32::MAX)
                .saturating_mul(mss)
        } else {
            u32::try_from((u64::from(ctx.pipe) * CUBIC_BETA) >> CUBIC_SHIFT).unwrap_or(u32::MAX)
        }
    }

    /// `cubic_cong_signal`: duplicate-ACK, ECN, RTO and RTO-error handling.
    pub fn congestion(&mut self, ctx: &CubicContext, signal: CubicSignal) {
        let mss = self.effective_mss(ctx);
        self.last_pipe = ctx.pipe;
        match signal {
            CubicSignal::DupAck => {
                self.flags &= !(CUBICFLAG_HYSTART_ENABLED | CUBICFLAG_HYSTART_IN_CSS);
                if !self.in_recovery {
                    let ssthresh = self.get_ssthresh(ctx).max(2 * mss);
                    self.ssthresh = ssthresh;
                    self.flags |= CUBICFLAG_CONG_EVENT;
                }
                self.in_recovery = true;
            }
            CubicSignal::Ecn => {
                self.flags &= !(CUBICFLAG_HYSTART_ENABLED | CUBICFLAG_HYSTART_IN_CSS);
                if !self.in_recovery {
                    let ssthresh = self.get_ssthresh(ctx).max(2 * mss);
                    self.ssthresh = ssthresh;
                    self.cwnd = ssthresh.max(mss);
                    self.flags |= CUBICFLAG_CONG_EVENT;
                    self.in_recovery = true;
                }
            }
            CubicSignal::Rto => {
                let shift = if ctx.rxtshift != 0 {
                    ctx.rxtshift
                } else {
                    self.rxtshift
                };
                if shift <= 1 {
                    self.undo_t_epoch = self.t_epoch;
                    self.undo_cwnd_epoch = self.cwnd_epoch;
                    self.undo_w_max = self.w_max;
                    self.undo_k = self.k;
                    let pipe = ctx.snd_wnd.min(ctx.pipe);
                    let reduced =
                        u32::try_from((u64::from(pipe) * CUBIC_BETA) >> CUBIC_SHIFT).unwrap_or(0);
                    self.ssthresh = (reduced / mss.max(1)).max(2).saturating_mul(mss);
                }
                self.flags |= CUBICFLAG_RTO_EVENT;
                self.cwnd = mss;
            }
            CubicSignal::RtoErr => {
                self.flags &= !CUBICFLAG_RTO_EVENT;
                self.k = self.undo_k;
                self.w_max = self.undo_w_max;
                self.cwnd_epoch = self.undo_cwnd_epoch;
                self.t_epoch = self.undo_t_epoch;
            }
        }
    }

    /// `cubic_post_recovery`: recovery-exit window and epoch RTT average.
    pub fn post_recovery(&mut self, ctx: &CubicContext) {
        let mss = self.effective_mss(ctx);
        let pipe = ctx.pipe;
        if self.in_recovery {
            if pipe < self.ssthresh {
                self.cwnd = pipe.max(mss).saturating_add(mss);
            } else {
                self.cwnd = u32::try_from((u64::from(self.w_max) * CUBIC_BETA) >> CUBIC_SHIFT)
                    .unwrap_or(u32::MAX)
                    .max(2 * mss);
            }
        }
        if self.epoch_ack_count > 0 && self.sum_rtt_us >= i64::from(self.epoch_ack_count) {
            self.mean_rtt_us = i32::try_from(self.sum_rtt_us / i64::from(self.epoch_ack_count))
                .unwrap_or(i32::MAX);
        }
        self.epoch_ack_count = 0;
        self.sum_rtt_us = 0;
    }

    /// `cubic_after_idle`: reset the restart window and curve epoch.
    pub fn after_idle(&mut self, now_us: i64) {
        let mss = self.mss;
        self.w_max = self.w_max.max(self.cwnd);
        self.k = Self::cubic_k(self.w_max / mss, self.cwnd_epoch / mss);
        if self.flags & CUBICFLAG_HYSTART_ENABLED == 0 {
            self.flags &= !CUBICFLAG_HYSTART_IN_CSS;
            self.flags |= CUBICFLAG_HYSTART_ENABLED;
        }
        let restart = (CUBIC_INITCWND_SEGMENTS * mss).min((2 * mss).max(10 * 1_460));
        self.ssthresh = self.ssthresh.max(self.cwnd.saturating_sub(self.cwnd >> 2));
        self.cwnd = restart.min(self.cwnd);
        self.t_epoch = now_us;
    }

    /// `cubic_newround`: rotate the Hystart++ round and leave CSS when due.
    pub fn newround(&mut self, round_cnt: u32, can_sh_cwnd: bool, cons_ssth: bool) {
        self.css_lastround_minrtt = self.css_current_round_minrtt;
        self.css_current_round_minrtt = CSS_INFINITY;
        self.css_rttsample_count = 0;
        self.css_current_round = round_cnt;
        if self.flags & CUBICFLAG_HYSTART_IN_CSS != 0
            && round_cnt.saturating_sub(self.css_entered_at_round) >= HYSTART_CSS_ROUNDS
        {
            if can_sh_cwnd {
                if cons_ssth {
                    self.ssthresh = (self.css_lowrtt_fas + self.css_fas_at_css_entry) / 2;
                } else {
                    self.ssthresh = self.css_lowrtt_fas;
                }
                self.cwnd = self.css_fas_at_css_entry;
                self.css_entered_at_round = round_cnt;
            } else {
                self.ssthresh = self.cwnd;
                self.flags &= !CUBICFLAG_HYSTART_IN_CSS;
                self.flags &= !CUBICFLAG_HYSTART_ENABLED;
            }
        }
    }

    /// `cubic_rttsample`: record one Hystart++ RTT sample.
    pub fn rttsample(&mut self, usec_rtt: u32, rxtcnt: u32, fas: u32) {
        if rxtcnt > 1 {
            return;
        }
        self.css_rttsample_count = self.css_rttsample_count.saturating_add(1);
        self.css_last_fas = fas;
        if self.css_current_round_minrtt > usec_rtt {
            self.css_current_round_minrtt = usec_rtt;
            self.css_lowrtt_fas = self.css_last_fas;
        }
        if self.css_rttsample_count >= HYSTART_N_RTTSAMPLES
            && self.css_current_round_minrtt != CSS_INFINITY
            && self.css_current_round_minrtt < self.css_baseline_minrtt
            && self.css_lastround_minrtt != CSS_INFINITY
        {
            self.flags &= !CUBICFLAG_HYSTART_IN_CSS;
            self.css_baseline_minrtt = CSS_INFINITY;
        }
    }
}

impl CongestionControl for Cubic {
    fn name(&self) -> &'static str {
        "cubic"
    }
    fn on_ack(&mut self, ctx: &mut CcContext, acked: u32, rtt: Option<Duration>) {
        let srtt_ms = rtt.map_or_else(
            || u32::try_from(self.mean_rtt_us / 1_000).unwrap_or(1).max(1),
            |value| u32::try_from(value.as_millis()).unwrap_or(u32::MAX).max(1),
        );
        if rtt.is_some() {
            self.rtt_updated = self.rtt_updated.saturating_add(1);
        }
        // `CcContext` carries no wall clock; the adapter advances a logical clock by one SRTT
        // per ACK. This is the documented interface-bound divergence; the donor path is the
        // explicit `CubicContext::now_us` supplied by tests and future wiring.
        self.clock_us = self
            .clock_us
            .saturating_add(i64::from(srtt_ms).saturating_mul(1_000));
        let mss = if ctx.mss == 0 {
            self.mss
        } else {
            ctx.mss.max(1)
        };
        let cubic = CubicContext {
            now_us: self.clock_us,
            srtt_ms,
            mss,
            snd_wnd: ctx.flight.max(self.cwnd),
            cwnd: self.cwnd,
            pipe: ctx.pipe,
            acked,
            nsegs: acked
                .saturating_add(mss.saturating_sub(1))
                .checked_div(mss)
                .unwrap_or(1)
                .max(1),
            snd_nxt: ctx.snd_nxt,
            snd_max: ctx.snd_nxt,
            // `CcContext` does not carry `CCF_CWND_LIMITED`; treat the sender as cwnd-limited
            // when the window is filled to within one segment, which keeps bulk transfers
            // growing while leaving small application-limited writes alone.
            cwnd_limited: ctx.flight.saturating_add(mss) >= self.cwnd,
            rtt_updated: self.rtt_updated,
            rxtshift: self.rxtshift,
            hystart_allowed: false,
        };
        self.ack_received(&cubic);
        // `ctx.pipe` arrives before `snd_una` advances; remove the ACKed bytes so the
        // recovery-exit decision sees the donated pipe estimate, not the pre-ACK flight.
        self.last_pipe = self.last_pipe.saturating_sub(acked);
        self.rxtshift = 0;
    }
    fn on_loss(&mut self, ctx: &mut CcContext, kind: LossKind) {
        let cubic = CubicContext {
            now_us: self.clock_us,
            srtt_ms: u32::try_from(self.mean_rtt_us / 1_000).unwrap_or(1).max(1),
            mss: ctx.mss,
            snd_wnd: ctx.flight,
            cwnd: self.cwnd,
            pipe: ctx.pipe,
            acked: 0,
            nsegs: 1,
            snd_nxt: ctx.snd_nxt,
            snd_max: ctx.snd_nxt,
            cwnd_limited: true,
            rtt_updated: self.rtt_updated,
            rxtshift: self.rxtshift,
            hystart_allowed: false,
        };
        let signal = if kind == LossKind::Ecn {
            CubicSignal::Ecn
        } else {
            CubicSignal::DupAck
        };
        self.congestion(&cubic, signal);
        // The live framework has no PRR stage; apply the recovery window at entry.
        self.cwnd = self.ssthresh;
    }
    fn on_rto(&mut self, ctx: &mut CcContext) {
        self.rxtshift = self.rxtshift.saturating_add(1);
        let cubic = CubicContext {
            now_us: self.clock_us,
            srtt_ms: u32::try_from(self.mean_rtt_us / 1_000).unwrap_or(1).max(1),
            mss: ctx.mss,
            snd_wnd: ctx.flight,
            cwnd: self.cwnd,
            pipe: ctx.pipe,
            acked: 0,
            nsegs: 1,
            snd_nxt: ctx.snd_nxt,
            snd_max: ctx.snd_nxt,
            cwnd_limited: true,
            rtt_updated: self.rtt_updated,
            rxtshift: self.rxtshift,
            hystart_allowed: false,
        };
        self.congestion(&cubic, CubicSignal::Rto);
        // FreeBSD `cc_do_congestion_signal` clears fast/con recovery on `CC_RTO`; the timer
        // owns that transition, so mirror it in the adapter.
        self.in_recovery = false;
    }
    fn cwnd(&self) -> u32 {
        self.cwnd
    }
    fn ssthresh(&self) -> u32 {
        self.ssthresh
    }
    fn in_recovery(&self) -> bool {
        self.in_recovery
    }
    fn on_recovery_exit(&mut self) {
        if self.in_recovery {
            let cubic = CubicContext {
                now_us: self.clock_us,
                srtt_ms: u32::try_from(self.mean_rtt_us / 1_000).unwrap_or(1).max(1),
                mss: self.mss,
                snd_wnd: self.cwnd,
                cwnd: self.cwnd,
                pipe: self.last_pipe,
                rtt_updated: self.rtt_updated,
                rxtshift: self.rxtshift,
                ..CubicContext::default()
            };
            self.post_recovery(&cubic);
            self.in_recovery = false;
        }
    }
    fn undo(&mut self, cwnd: u32, ssthresh: u32) {
        self.cwnd = cwnd;
        self.ssthresh = ssthresh;
        self.in_recovery = false;
    }
}

/// Integer floor cube root via binary search.
#[must_use]
pub fn icbrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut low = 1u128;
    let mut high = 1u128;
    while high.saturating_mul(high).saturating_mul(high) <= value {
        high = high.saturating_mul(2);
    }
    while low + 1 < high {
        let mid = (low + high) / 2;
        if mid.saturating_mul(mid).saturating_mul(mid) <= value {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

/// Proportional Rate Reduction (RFC 6937) recovery accounting.
#[derive(Clone, Debug, Default)]
pub struct Prr {
    ssthresh: u32,
    recover_fs: u32,
    delivered: u32,
    out: u32,
    mss: u32,
}

impl Prr {
    /// Enter recovery.
    #[must_use]
    pub fn enter(ssthresh: u32, recover_fs: u32, mss: u32) -> Self {
        Self {
            ssthresh,
            recover_fs: recover_fs.max(1),
            delivered: 0,
            out: 0,
            mss,
        }
    }

    /// Account for an ACK and return how many bytes may be sent.
    pub fn on_ack(&mut self, delivered_data: u32, pipe: u32) -> u32 {
        self.delivered = self.delivered.saturating_add(delivered_data);
        if pipe > self.ssthresh {
            let target = (u128::from(self.delivered) * u128::from(self.ssthresh))
                .div_ceil(u128::from(self.recover_fs));
            u32::try_from(target)
                .unwrap_or(u32::MAX)
                .saturating_sub(self.out)
        } else {
            let limit = self
                .delivered
                .saturating_sub(self.out)
                .max(delivered_data)
                .saturating_add(self.mss);
            self.ssthresh.saturating_sub(pipe).min(limit)
        }
    }

    /// Record transmitted bytes.
    pub fn sent(&mut self, bytes: u32) {
        self.out = self.out.saturating_add(bytes);
    }
}

#[cfg(test)]
#[path = "cc_ut.rs"]
mod tests;
