// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// The backoff table, `TCP_REXMTVAL` and `TCPT_RANGESET` derivations are ported from FreeBSD
// `sys/netinet/tcp_timer.c`/`tcp_var.h`, Copyright (c) 1982, 1986, 1993
// The Regents of the University of California. BSD-3-Clause.

//! RFC 6298 RTT estimator in integer q8 arithmetic.
//!
//! The retransmission backoff table, the `TCP_REXMTVAL` base and the range clamp are ported
//! from FreeBSD `sys/netinet/tcp_timer.c` (`tcp_backoff`, `tcp_rexmit_*`), `sys/netinet/tcp_timer.h`
//! (`TCP_MAXRXTSHIFT`, `TCP_RTT_INVALIDATE`, `TCPTV_MIN`, `TCPTV_CPU_VAR`, `TCPTV_REXMTMAX`,
//! `TCPT_RANGESET`) and `sys/netinet/tcp_var.h` (`TCP_REXMTVAL`), at donor revision
//! `133344405f532bd54ff3629a77d4c4e062f01fdb`.

/// FreeBSD `tcp_backoff[]`: RTO multiplier indexed by `t_rxtshift`.
pub const TCP_BACKOFF: [u32; 13] = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 512, 512, 512];

/// FreeBSD `tcp_totbackoff`: the sum of [`TCP_BACKOFF`].
// The persist timer itself is K265/Jerry.
#[allow(dead_code)]
pub const TCP_TOTBACKOFF: u32 = 2_559;

/// FreeBSD `TCP_MAXRXTSHIFT`: the last retransmit shift before a drop.
pub const TCP_MAXRXTSHIFT: u8 = 12;

/// FreeBSD `TCP_RTT_INVALIDATE`: after this shift the next sample starts a new estimate.
pub const TCP_RTT_INVALIDATE: u8 = TCP_MAXRXTSHIFT / 4;

/// FreeBSD `TCPTV_REXMTMAX` in milliseconds (64 s).
pub const TCPTV_REXMTMAX_MS: u64 = 64_000;

/// FreeBSD `TCPTV_MIN` in milliseconds (30 ms).
pub const TCPTV_MIN_MS: u64 = 30;

/// FreeBSD `TCPTV_CPU_VAR` in milliseconds: the `TCPT_RANGESET` processing slop (200 ms).
pub const TCPTV_CPU_VAR_MS: u64 = 200;

/// FreeBSD `TCPTV_RTOBASE` in milliseconds: the initial RTO (1 s).
pub const TCPTV_RTOBASE_MS: u64 = 1_000;

/// Fixed-point RTO estimator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RtoEstimator {
    srtt_q8: u64,
    rttvar_q8: u64,
    has_sample: bool,
    srtt_invalid: bool,
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
            srtt_invalid: false,
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
        if !self.has_sample || self.srtt_invalid {
            // FreeBSD `tcp_xmit_timer` takes this branch when `t_srtt == 0` or
            // `t_rxtshift > TCP_RTT_INVALIDATE`.
            self.srtt_q8 = rq;
            self.rttvar_q8 = rq / 2;
            self.has_sample = true;
            self.srtt_invalid = false;
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

    /// FreeBSD `TCP_REXMTVAL(tp)` in milliseconds: the RTO base before backoff.
    ///
    /// FreeBSD seeds `t_rttvar` so a connection without a sample yields `TCPTV_RTOBASE` (1 s);
    /// after a sample the value is `srtt + 4 * rttvar`. Unlike [`Self::rto_ms`] this value is not
    /// changed by [`Self::timeout`]; the retransmit timer applies the backoff itself.
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

    /// FreeBSD `TCPT_RANGESET`: force `value` into the `[min, max]` range.
    #[must_use]
    pub const fn rangeset(value: u64, min: u64, max: u64) -> u64 {
        let value = if value < min { min } else { value };
        if value > max { max } else { value }
    }

    /// FreeBSD `tcp_timer_rexmt` RTO arm:
    /// `RANGESET(base * tcp_backoff[shift] + tcp_rexmit_slop, t_rttmin, tcp_rexmit_max)`.
    #[must_use]
    pub fn backoff_from_ms(
        base_ms: u64,
        shift: u8,
        rttmin_ms: u64,
        slop_ms: u64,
        max_ms: u64,
    ) -> u64 {
        let base = base_ms.max(rttmin_ms);
        let factor = u64::from(TCP_BACKOFF[usize::from(shift.min(TCP_MAXRXTSHIFT))]);
        Self::rangeset(
            base.saturating_mul(factor).saturating_add(slop_ms),
            rttmin_ms,
            max_ms,
        )
    }

    /// Arm from [`Self::rexmtval_ms`]; see [`Self::backoff_from_ms`].
    #[must_use]
    pub fn backoff_ms(&self, shift: u8, rttmin_ms: u64, slop_ms: u64, max_ms: u64) -> u64 {
        Self::backoff_from_ms(self.rexmtval_ms(), shift, rttmin_ms, slop_ms, max_ms)
    }

    /// FreeBSD `TCP_RTT_INVALIDATE`: make the next sample start a new estimate.
    ///
    /// FreeBSD does not fold srtt into rttvar in `tcp_timer_rexmt`; it calls `in_losing()` and
    /// `tcp_xmit_timer` takes its first-measurement branch while `t_rxtshift` is still above
    /// `TCP_RTT_INVALIDATE`.
    pub fn invalidate_srtt(&mut self) {
        self.srtt_invalid = true;
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
