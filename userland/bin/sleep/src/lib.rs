// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure argument parsing for `sleep`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::fmt;

/// Largest accepted seconds value; keeps the nanosecond conversion representable.
pub const MAX_SECONDS: u64 = u64::MAX / 1_000_000_000;

/// The command line was not a valid `sleep` invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageError;

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("usage: sleep <seconds> | sleep --selftest")
    }
}

/// Parse a whole number of seconds.
pub fn parse_seconds(text: &str) -> Result<u64, UsageError> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(UsageError);
    }
    let seconds = text.parse::<u64>().map_err(|_| UsageError)?;
    if seconds > MAX_SECONDS {
        return Err(UsageError);
    }
    Ok(seconds)
}

/// Convert seconds to nanoseconds, or `None` when the product would overflow.
pub fn to_nanos(seconds: u64) -> Option<u64> {
    seconds.checked_mul(1_000_000_000)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
