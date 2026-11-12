// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RFC 6298 RTT estimator in integer q8 arithmetic.

/// Fixed-point RTO estimator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RtoEstimator {
    srtt_q8: u64,
    rttvar_q8: u64,
    has_sample: bool,
    rto_ms: u64,
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
        if !self.has_sample {
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
