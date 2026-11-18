// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The one-shot/interval alarm state machine.
//!
//! The shell owns the kernel timer and the SIGALRM delivery thread; this crate owns the pure
//! deadline arithmetic and the POSIX "remaining time of the previous request" return value.

/// One process alarm: a deadline, an optional repeat interval and whether it is armed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Alarm {
    deadline_ns: u64,
    interval_ns: u64,
    active: bool,
}

impl Alarm {
    /// An unarmed alarm.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            deadline_ns: 0,
            interval_ns: 0,
            active: false,
        }
    }

    /// Arm one request and return the previous remaining time in nanoseconds.
    ///
    /// A zero `initial_ns` cancels the request. The interval is zero for `alarm(2)`.
    pub fn set(&mut self, now_ns: u64, initial_ns: u64) -> u64 {
        self.set_interval(now_ns, initial_ns, 0)
    }

    /// Arm one request with an optional repeat interval and return the previous remaining time.
    ///
    /// A zero `initial_ns` cancels. A non-zero `interval_ns` repeats every interval.
    pub fn set_interval(&mut self, now_ns: u64, initial_ns: u64, interval_ns: u64) -> u64 {
        let previous = self.remaining_ns(now_ns);
        self.deadline_ns = now_ns.saturating_add(initial_ns);
        self.interval_ns = interval_ns;
        self.active = initial_ns != 0;
        previous
    }

    /// The time left before the next expiry, in nanoseconds.
    #[must_use]
    pub fn remaining_ns(&self, now_ns: u64) -> u64 {
        if !self.active || now_ns >= self.deadline_ns {
            0
        } else {
            self.deadline_ns.saturating_sub(now_ns)
        }
    }

    /// Whether a request is armed.
    #[must_use]
    pub const fn active(&self) -> bool {
        self.active
    }

    /// The repeat interval in nanoseconds, or zero for a one-shot request.
    #[must_use]
    pub const fn interval_ns(&self) -> u64 {
        self.interval_ns
    }

    /// The absolute monotonic deadline in nanoseconds.
    #[must_use]
    pub const fn deadline_ns(&self) -> u64 {
        self.deadline_ns
    }
}

#[cfg(test)]
#[path = "alarm_ut.rs"]
mod tests;
