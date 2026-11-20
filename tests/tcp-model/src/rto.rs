// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RFC 6298 / NetBSD RTO and persist model.
//!
//! Behaviour reference: NetBSD `sys/netinet/tcp_input.c,v 1.441` `tcp_xmit_timer` and
//! `sys/netinet/tcp_timer.c,v 1.99` `tcp_timer_rexmt`/`tcp_timer_persist`; RFC 6298.
//! `RtoModel` uses floating point for the reference, `NaiveRto` uses integer microseconds.

/// Floating-point RFC 6298 estimator.
#[derive(Clone, Copy, Debug)]
pub struct RtoModel {
    /// Smoothed RTT in milliseconds.
    pub srtt: f64,
    /// RTT variation in milliseconds.
    pub rttvar: f64,
    /// Current RTO in milliseconds.
    pub rto_ms: f64,
    /// Minimum RTO.
    pub min_ms: f64,
    /// Maximum RTO.
    pub max_ms: f64,
    /// Current backoff shift.
    pub backoff: u32,
    /// Number of RTT samples.
    pub samples: u32,
}

impl RtoModel {
    /// Create a model with the initial RTO clamped to `[min,max]`.
    #[must_use]
    pub fn new(initial_ms: f64, min_ms: f64, max_ms: f64) -> Self {
        Self {
            srtt: 0.0,
            rttvar: 0.0,
            rto_ms: initial_ms.clamp(min_ms, max_ms),
            min_ms,
            max_ms,
            backoff: 0,
            samples: 0,
        }
    }

    /// Feed one RTT sample.
    pub fn sample(&mut self, rtt_ms: f64) {
        let rtt = rtt_ms.max(0.0);
        if self.samples == 0 {
            self.srtt = rtt;
            self.rttvar = rtt / 2.0;
        } else {
            self.rttvar = 0.75 * self.rttvar + 0.25 * (self.srtt - rtt).abs();
            self.srtt = 0.875 * self.srtt + 0.125 * rtt;
        }
        self.samples = self.samples.saturating_add(1);
        self.backoff = 0;
        self.rto_ms = (self.srtt + 4.0 * self.rttvar).clamp(self.min_ms, self.max_ms);
    }

    /// Apply a timeout: double and clamp.
    pub fn timeout(&mut self) {
        self.backoff = self.backoff.saturating_add(1);
        self.rto_ms = (self.rto_ms * 2.0).min(self.max_ms);
    }

    /// Persist/retransmit backoff delay in milliseconds for the current shift.
    #[must_use]
    pub fn backoff_delay_ms(&self) -> u64 {
        persist_delay_ms(self.backoff)
    }
}

/// Integer microsecond reference.
#[derive(Clone, Copy, Debug)]
pub struct NaiveRto {
    /// SRTT in microseconds.
    pub srtt_us: i64,
    /// RTTVAR in microseconds.
    pub rttvar_us: i64,
    /// RTO in microseconds.
    pub rto_us: i64,
    /// Minimum RTO.
    pub min_us: i64,
    /// Maximum RTO.
    pub max_us: i64,
    /// Samples taken.
    pub samples: u32,
}

impl NaiveRto {
    /// Create the integer reference.
    #[must_use]
    pub fn new(initial_ms: u64, min_ms: u64, max_ms: u64) -> Self {
        Self {
            srtt_us: 0,
            rttvar_us: 0,
            rto_us: i64::try_from(initial_ms.saturating_mul(1_000))
                .unwrap_or(i64::MAX)
                .clamp(
                    i64::try_from(min_ms.saturating_mul(1_000)).unwrap_or(i64::MAX),
                    i64::try_from(max_ms.saturating_mul(1_000)).unwrap_or(i64::MAX),
                ),
            min_us: i64::try_from(min_ms.saturating_mul(1_000)).unwrap_or(i64::MAX),
            max_us: i64::try_from(max_ms.saturating_mul(1_000)).unwrap_or(i64::MAX),
            samples: 0,
        }
    }

    /// Feed one sample in milliseconds.
    pub fn sample(&mut self, rtt_ms: u64) {
        let rtt = i64::try_from(rtt_ms.saturating_mul(1_000)).unwrap_or(i64::MAX);
        if self.samples == 0 {
            self.srtt_us = rtt;
            self.rttvar_us = rtt / 2;
        } else {
            self.rttvar_us = (3 * self.rttvar_us + (self.srtt_us - rtt).abs()) / 4;
            self.srtt_us = (7 * self.srtt_us + rtt) / 8;
        }
        self.samples = self.samples.saturating_add(1);
        self.rto_us = (self.srtt_us + 4 * self.rttvar_us).clamp(self.min_us, self.max_us);
    }

    /// Apply a timeout.
    pub fn timeout(&mut self) {
        self.rto_us = (self.rto_us.saturating_mul(2)).min(self.max_us);
    }

    /// Current RTO in milliseconds, rounded up.
    #[must_use]
    pub fn rto_ms(&self) -> u64 {
        u64::try_from((self.rto_us + 999) / 1_000).unwrap_or(u64::MAX)
    }
}

/// NetBSD `tcp_backoff`/persist delay schedule in milliseconds.
#[must_use]
pub const fn persist_delay_ms(shift: u32) -> u64 {
    let seconds = match shift {
        0 => 1,
        1 => 2,
        2 => 4,
        3 => 8,
        4 => 16,
        5 => 32,
        _ => 64,
    };
    seconds * 1_000
}
