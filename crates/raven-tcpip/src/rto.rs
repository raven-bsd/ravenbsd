// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// The backoff table, `TCP_REXMTVAL` and `TCPT_RANGESET` derivations are ported from NetBSD
// `sys/netinet/tcp_timer.c`/`tcp_timer.h`/`tcp_var.h`, Copyright (c) 1982, 1986, 1993
// The Regents of the University of California, and Copyright (c) 1997, 1998, 2001, 2005
// The NetBSD Foundation, Inc. BSD-3-Clause.

//! NetBSD RTT/RTO fixed-point estimator and retransmission backoff.
//!
//! The retransmission backoff table, the `TCP_REXMTVAL` base and the range clamp are ported
//! from NetBSD `sys/netinet/tcp_timer.c` (`tcp_backoff`, `tcp_totbackoff`), `sys/netinet/tcp_timer.h`
//! (`TCP_MAXRXTSHIFT`, `TCPTV_MIN`, `TCPTV_REXMTMAX`, `TCPT_RANGESET`) and
//! `sys/netinet/tcp_var.h` (`TCP_REXMTVAL`, `TCP_RTT_SHIFT`, `TCP_RTTVAR_SHIFT`), at donor
//! revision `36eddebcc36cb0775aa2962005b9c05419a6ff01`. The sample update keeps the existing
//! RFC 2988 q8 type (the NetBSD `tcp_xmit_timer` caller is `tcp_input.c`, outside this ticket).

/// NetBSD `tcp_backoff[]`: RTO multiplier indexed by `t_rxtshift`.
pub const TCP_BACKOFF: [u32; 13] = [1, 2, 4, 8, 16, 32, 64, 64, 64, 64, 64, 64, 64];

/// NetBSD `tcp_totbackoff`: the sum of [`TCP_BACKOFF`].
// The persist timer itself is K254/Jerry.
#[allow(dead_code)]
pub const TCP_TOTBACKOFF: u32 = 511;

/// NetBSD `TCP_MAXRXTSHIFT`: the last retransmit shift before a drop.
pub const TCP_MAXRXTSHIFT: u8 = 12;

/// NetBSD `TCP_RTT_INVALIDATE` role: after this shift the srtt is moved into rttvar.
pub const TCP_RTT_INVALIDATE: u8 = TCP_MAXRXTSHIFT / 4;

/// NetBSD `TCPTV_REXMTMAX` in milliseconds (64 slow ticks, 32 s).
pub const TCPTV_REXMTMAX_MS: u64 = 32_000;

/// NetBSD `TCPTV_MIN` in milliseconds (one slow tick, 500 ms).
pub const TCPTV_MIN_MS: u64 = 500;

/// NetBSD `TCPTV_SRTTDFLT` in milliseconds: the initial RTO (1 s).
pub const TCPTV_RTOBASE_MS: u64 = 1_000;

/// Fixed-point RTO estimator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RtoEstimator {
    srtt_q8: u64,
    rttvar_q8: u64,
    has_sample: bool,
    rto_ms: u64,
    initial_ms: u64,
    last_computed_ms: u64,
    rto_min_ms: u64,
    rto_max_ms: u64,
    clock_granularity_ms: u64,
}

impl RtoEstimator {
    /// Create an estimator with the configured bounds and initial RTO.
    #[must_use]
    pub const fn new(initial_ms: u64, min_ms: u64, max_ms: u64, clock_granularity_ms: u64) -> Self {
        let rto = if initial_ms < min_ms {
            min_ms
        } else if initial_ms > max_ms {
            max_ms
        } else {
            initial_ms
        };
        Self {
            srtt_q8: 0,
            rttvar_q8: 0,
            has_sample: false,
            rto_ms: rto,
            initial_ms: rto,
            last_computed_ms: rto,
            rto_min_ms: min_ms,
            rto_max_ms: max_ms,
            clock_granularity_ms: if clock_granularity_ms == 0 {
                1
            } else {
                clock_granularity_ms
            },
        }
    }

    /// Feed one RTT sample in whole milliseconds.
    pub fn sample(&mut self, rtt_ms: u64) {
        let rq = rtt_ms.min(86_400_000).saturating_mul(8);
        if !self.has_sample || self.srtt_q8 == 0 {
            // NetBSD `tcp_xmit_timer` takes this branch when `t_srtt == 0`; the first sample
            // sets srtt = R and rttvar = R / 2 (RFC 2988 2.2).
            self.srtt_q8 = rq;
            self.rttvar_q8 = rq / 2;
            self.has_sample = true;
        } else {
            let diff = self.srtt_q8.abs_diff(rq);
            self.rttvar_q8 = ceil_div(self.rttvar_q8.saturating_mul(3).saturating_add(diff), 4);
            self.srtt_q8 = ceil_div(self.srtt_q8.saturating_mul(7).saturating_add(rq), 8);
        }
        let gq = self.clock_granularity_ms.saturating_mul(8);
        let margin = gq.max(self.rttvar_q8.saturating_mul(4));
        let computed = ceil_div(self.srtt_q8.saturating_add(margin), 8);
        self.last_computed_ms = computed;
        self.rto_ms = computed.clamp(self.rto_min_ms, self.rto_max_ms);
    }

    /// Apply an RTO timeout: double and clamp, without changing SRTT/RTTVAR.
    pub fn timeout(&mut self) {
        self.rto_ms = self.rto_ms.saturating_mul(2).min(self.rto_max_ms);
    }

    /// The armed RTO in milliseconds.
    #[must_use]
    pub const fn rto_ms(&self) -> u64 {
        self.rto_ms
    }

    /// SRTT rounded up to whole milliseconds.
    #[must_use]
    pub const fn srtt_ms(&self) -> u64 {
        ceil_div(self.srtt_q8, 8)
    }

    /// RTTVAR rounded up to whole milliseconds.
    #[must_use]
    pub const fn rttvar_ms(&self) -> u64 {
        ceil_div(self.rttvar_q8, 8)
    }

    /// The formula result before the configured clamp.
    #[must_use]
    pub const fn rto_computed_ms(&self) -> u64 {
        self.last_computed_ms
    }

    /// NetBSD `TCP_REXMTVAL(tp)`: the RTO base before backoff.
    ///
    /// NetBSD returns `srtt + 4 * rttvar` in slow ticks; the q8 type returns the same value in
    /// milliseconds. Unlike [`Self::rto_ms`] this value is not changed by [`Self::timeout`];
    /// the retransmit timer applies the backoff itself.
    #[must_use]
    pub const fn rexmtval_ms(&self) -> u64 {
        if self.has_sample {
            ceil_div(
                self.srtt_q8
                    .saturating_add(self.rttvar_q8.saturating_mul(4)),
                8,
            )
        } else {
            self.initial_ms
        }
    }

    /// NetBSD `TCPT_RANGESET`: force `value` into the `[min, max]` range.
    #[must_use]
    pub const fn rangeset(value: u64, min: u64, max: u64) -> u64 {
        let value = if value < min { min } else { value };
        if value > max { max } else { value }
    }

    /// NetBSD `tcp_timer_rexmt` RTO arm:
    /// `TCPT_RANGESET(rxtcur, max(TCP_REXMTVAL, t_rttmin) * tcp_backoff[t_rxtshift], t_rttmin,
    /// TCPTV_REXMTMAX)`.
    #[must_use]
    pub fn backoff(base_ms: u64, shift: u8, rttmin_ms: u64, max_ms: u64) -> u64 {
        let base = base_ms.max(rttmin_ms);
        let factor = u64::from(TCP_BACKOFF[usize::from(shift.min(TCP_MAXRXTSHIFT))]);
        Self::rangeset(base.saturating_mul(factor), rttmin_ms, max_ms)
    }

    /// Arm from [`Self::rexmtval_ms`]; see [`Self::backoff`].
    #[must_use]
    pub fn backoff_ms(&self, shift: u8, rttmin_ms: u64, max_ms: u64) -> u64 {
        Self::backoff(self.rexmtval_ms(), shift, rttmin_ms, max_ms)
    }

    /// NetBSD `tcp_timer_rexmt` losing reset: `t_rttvar += t_srtt >> TCP_RTT_SHIFT;
    /// t_srtt = 0`.
    ///
    /// The q8 millisecond type keeps `srtt + 4 * rttvar` constant by adding `srtt / 4` to
    /// rttvar and zeroing srtt. The next sample sees `srtt == 0` and takes the
    /// first-measurement branch, as NetBSD's `t_srtt == 0` does.
    pub fn losing_reset(&mut self) {
        self.rttvar_q8 = self.rttvar_q8.saturating_add(self.srtt_q8 / 4);
        self.srtt_q8 = 0;
    }

    /// The clamped initial RTO in milliseconds.
    #[must_use]
    pub const fn initial_ms(&self) -> u64 {
        self.initial_ms
    }

    /// Raw SRTT in q8.
    #[must_use]
    pub const fn srtt_q8(&self) -> u64 {
        self.srtt_q8
    }

    /// Raw RTTVAR in q8.
    #[must_use]
    pub const fn rttvar_q8(&self) -> u64 {
        self.rttvar_q8
    }

    /// Whether at least one sample has been taken.
    #[must_use]
    pub const fn has_sample(&self) -> bool {
        self.has_sample
    }
}

#[allow(clippy::manual_is_multiple_of)]
const fn ceil_div(value: u64, divisor: u64) -> u64 {
    if divisor == 0 {
        return value;
    }
    value / divisor + if value % divisor != 0 { 1 } else { 0 }
}
