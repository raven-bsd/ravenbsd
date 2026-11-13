// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `FmtErr` -> return value / `errno` table of `donor-map/04` 4.6 (C43), one function so the
//! host test covers every variant.

use raven_libc_fmt::FmtErr;

use super::errno;
use raven_libc_abi::errno::Errno;

/// The C return value of a failed printf/scanf call; sets `errno` where the table says so.
pub(crate) fn apply(error: FmtErr) -> i32 {
    match error {
        FmtErr::Invalid => {
            errno::set(Errno(22));
            -1
        }
        FmtErr::NoMem => {
            errno::set(Errno(12));
            -1
        }
        FmtErr::Overflow => {
            errno::set(Errno(75));
            -1
        }
        FmtErr::Encoding => {
            errno::set(Errno(84));
            -1
        }
        FmtErr::PctNAbort => {
            errno::set(Errno(22));
            -1
        }
        // A sink failure already set `errno` in the write path; never overwrite it.
        FmtErr::Sink | FmtErr::FloatUnavailable => -1,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "map_fmt_ut.rs"]
mod tests;
