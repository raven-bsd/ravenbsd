// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: CLEANROOM `utime` argument conversion, from the prose in
// architecture/raven-libc/donor-map/06-time-tz.md section 3.5 and POSIX.

//! The `[atime, mtime]` pair the filesystem shell hands to `utimensat`.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: microsecond conversion uses saturating multiplication.

use raven_libc_abi::structs::Timespec;

/// Build the `utimensat` pair.
///
/// `None` means "both times are now"; an explicit pair carries microseconds
/// and is converted to nanoseconds. The shell supplies `now`.
#[must_use]
pub fn utime_timespecs(buffer: Option<(i64, i64)>, now: Timespec) -> [Timespec; 2] {
    match buffer {
        None => [now, now],
        Some((seconds, microseconds)) => {
            let nanos = microseconds.saturating_mul(1_000);
            [
                Timespec {
                    tv_sec: seconds,
                    tv_nsec: nanos,
                },
                Timespec {
                    tv_sec: seconds,
                    tv_nsec: nanos,
                },
            ]
        }
    }
}

#[cfg(test)]
#[path = "utime_ut.rs"]
mod tests;
