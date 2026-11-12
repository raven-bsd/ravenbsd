// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Congestion-control framework: NewReno, CUBIC and PRR in integer arithmetic.

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
}

/// NewReno with RFC 3465 appropriate byte counting and RFC 6582 recovery.
#[derive(Clone, Debug)]
pub struct NewReno {
    cwnd: u32,
    ssthresh: u32,
    mss: u32,
    cwnd_max: u32,
    dup_threshold: u32,
    recover: u32,
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
        self.ssthresh = (flight / 2).max(2 * self.mss);
        self.cwnd = self.ssthresh.saturating_add(self.dup_threshold * self.mss);
        self.in_recovery = true;
        self.recover = snd_nxt;
    }

    /// Apply a partial ACK inside recovery.
    pub fn partial_ack(&mut self, acked: u32) {
        if !self.in_recovery {
            return;
        }
        let bonus = if acked >= self.mss { self.mss } else { 0 };
        self.cwnd = self
            .cwnd
            .saturating_sub(acked)
            .saturating_add(bonus)
            .max(2 * self.mss);
    }

    /// Apply a full ACK at or above `recover`.
    pub fn full_ack(&mut self) {
        if self.in_recovery {
            self.cwnd = self.ssthresh;
            self.in_recovery = false;
        }
    }

    /// Idle restart: RFC 5681 section 4.1.
    pub fn idle_restart(&mut self) {
        self.cwnd = self.cwnd.min(10 * self.mss);
    }
}

impl CongestionControl for NewReno {
    fn name(&self) -> &'static str {
        "newreno"
    }

    fn on_ack(&mut self, _ctx: &mut CcContext, acked_bytes: u32, _rtt: Option<Duration>) {
        if self.in_recovery {
            return;
        }
        if self.in_rto {
            self.in_rto = false;
        }
        let acked = acked_bytes.min(self.mss);
        if self.cwnd < self.ssthresh {
            self.cwnd = self.cwnd.saturating_add(acked);
        } else {
            let increment = ((self.mss as u64 * self.mss as u64) / self.cwnd as u64).max(1);
            self.cwnd = self
                .cwnd
                .saturating_add(u32::try_from(increment).unwrap_or(u32::MAX));
        }
        self.cwnd = self.cwnd.min(self.cwnd_max);
    }

    fn on_loss(&mut self, ctx: &mut CcContext, _kind: LossKind) {
        self.loss_fast_retransmit(ctx.flight, ctx.snd_nxt);
    }

    fn on_rto(&mut self, ctx: &mut CcContext) {
        if !self.in_rto {
            self.ssthresh = (ctx.flight / 2).max(2 * self.mss);
            self.in_rto = true;
        }
        self.cwnd = self.mss;
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
}

/// CUBIC (RFC 8312) with integer-only cube root and curve.
#[derive(Clone, Debug)]
pub struct Cubic {
    cwnd: u32,
    ssthresh: u32,
    mss: u32,
    cwnd_max: u32,
    w_max: u32,
    w_last_max: u32,
    w_max_segs: u128,
    k_us: u128,
    epoch_us: i128,
    idle_accum_us: i128,
    in_recovery: bool,
}

const BETA_NUM: u128 = 7;
const BETA_DEN: u128 = 10;
const CUBIC_C_NUM: u128 = 2;
const CUBIC_C_DEN: u128 = 5;
const W_EST_NUM: u128 = 9;
const W_EST_DEN: u128 = 17;

impl Cubic {
    /// Construct a CUBIC controller.
    #[must_use]
    pub fn new(mss: u32, initial_cwnd: u32, initial_ssthresh: u32, cwnd_max: u32) -> Self {
        Self {
            cwnd: initial_cwnd.max(2 * mss),
            ssthresh: initial_ssthresh,
            mss: mss.max(1),
            cwnd_max: cwnd_max.max(2 * mss),
            w_max: 0,
            w_last_max: 0,
            w_max_segs: 0,
            k_us: 0,
            epoch_us: 0,
            idle_accum_us: 0,
            in_recovery: false,
        }
    }

    /// Set the initial `W_last_max` for fast-convergence sequences.
    pub fn set_w_last_max(&mut self, bytes: u32) {
        self.w_last_max = bytes;
    }

    /// The current `W_max` in bytes.
    #[must_use]
    pub const fn w_max(&self) -> u32 {
        self.w_max
    }

    /// The current `K` in microseconds.
    #[must_use]
    pub const fn k_us(&self) -> u128 {
        self.k_us
    }

    /// Apply a congestion event at `now_ms`.
    pub fn loss_event(&mut self, now_ms: i64) {
        let pre = self.cwnd;
        self.w_max = if pre < self.w_last_max {
            u32::try_from(u128::from(pre) * 17 / 20).unwrap_or(pre)
        } else {
            pre
        };
        self.w_last_max = pre;
        self.ssthresh = ((u128::from(self.w_max) * BETA_NUM / BETA_DEN) as u32).max(2 * self.mss);
        self.cwnd = self.ssthresh;
        self.w_max_segs = u128::from(self.w_max) / u128::from(self.mss);
        self.k_us = icbrt(750_000_000_000_000_000u128.saturating_mul(self.w_max_segs));
        self.epoch_us = i128::from(now_ms) * 1000;
        self.idle_accum_us = 0;
        self.in_recovery = true;
    }

    /// Advance one RTT to `at_ms`, excluding `idle_ms` from the curve time.
    pub fn advance(&mut self, at_ms: i64, idle_ms: i64, rtt_ms: u64) {
        let effective_us = (i128::from(at_ms - idle_ms) * 1000)
            .saturating_sub(self.epoch_us)
            .saturating_sub(self.idle_accum_us);
        let t_us = u128::try_from(effective_us.max(0)).unwrap_or(0);
        let rtt_us = u128::from(rtt_ms).saturating_mul(1000).max(1);
        let w_cubic = if t_us >= self.k_us {
            let dt = t_us - self.k_us;
            let term =
                (CUBIC_C_NUM * dt.saturating_pow(3)) / (CUBIC_C_DEN * 1_000_000_000_000_000_000);
            self.w_max_segs.saturating_add(term)
        } else {
            let dt = self.k_us - t_us;
            let term =
                (CUBIC_C_NUM * dt.saturating_pow(3)) / (CUBIC_C_DEN * 1_000_000_000_000_000_000);
            self.w_max_segs.saturating_sub(term)
        };
        let w_est =
            (7 * self.w_max_segs / 10).saturating_add(W_EST_NUM * t_us / (W_EST_DEN * rtt_us));
        let target = w_cubic.max(w_est).max(2);
        let bytes = target.saturating_mul(u128::from(self.mss));
        self.cwnd = u32::try_from(bytes.min(u128::from(self.cwnd_max))).unwrap_or(self.cwnd_max);
        self.idle_accum_us = self
            .idle_accum_us
            .saturating_add(i128::from(idle_ms) * 1000);
    }

    /// RFC 8312 RTO response.
    pub fn rto_event(&mut self) {
        self.ssthresh = u32::try_from(u128::from(self.cwnd) * BETA_NUM / BETA_DEN)
            .unwrap_or(self.cwnd)
            .max(2 * self.mss);
        self.cwnd = self.mss;
        self.in_recovery = false;
    }
}

impl CongestionControl for Cubic {
    fn name(&self) -> &'static str {
        "cubic"
    }
    fn on_ack(&mut self, _ctx: &mut CcContext, acked: u32, _rtt: Option<Duration>) {
        if self.in_recovery {
            return;
        }
        let acked = acked.min(self.mss);
        if self.cwnd < self.ssthresh {
            self.cwnd = self.cwnd.saturating_add(acked);
        } else {
            let increment = ((self.mss as u64 * self.mss as u64) / self.cwnd as u64).max(1);
            self.cwnd = self
                .cwnd
                .saturating_add(u32::try_from(increment).unwrap_or(u32::MAX));
        }
        self.cwnd = self.cwnd.min(self.cwnd_max);
    }
    fn on_loss(&mut self, _ctx: &mut CcContext, _kind: LossKind) {
        self.loss_event(0);
    }
    fn on_rto(&mut self, _ctx: &mut CcContext) {
        self.rto_event();
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
