// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Sleep-list helpers used by `NANOSLEEP`, futex timeouts and kernel self-tests.

/// Monotonic nanoseconds.
pub fn now_ns() -> u64 {
    crate::time::now_ns()
}

/// Sleep until the monotonic `deadline`.
///
/// Returns zero when the deadline was reached, or `-ETIMEDOUT` when a caller-supplied timeout
/// won the race (not used by `NANOSLEEP` in S09).
pub fn sleep_until(deadline: u64) -> isize {
    super::sleep_until(deadline)
}

/// Sleep for at least `ns` nanoseconds. A zero delay is a yield.
pub fn sleep_ns(ns: u64) -> isize {
    if ns == 0 {
        super::yield_now();
        return 0;
    }
    let deadline = now_ns().saturating_add(ns);
    super::sleep_until(deadline)
}
