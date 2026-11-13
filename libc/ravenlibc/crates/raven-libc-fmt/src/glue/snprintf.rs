// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `vsnprintf` buffer semantics (CLEANROOM).
//!
//! Read for the contract only: `freebsd:lib/libc/stdio/vsnprintf.c` and
//! `musl:src/stdio/vsnprintf.c` (`donor-map/04` 5.2). The FreeBSD rule wins: `n == 0` is
//! legal with a NULL buffer and returns the full length; `n > 0` stores at most `n - 1`
//! units, always NUL-terminates, and returns the length that would have been written;
//! `n - 1 > INT_MAX` is [`FmtErr::Overflow`] with `out[0] = 0`.

use crate::printf::vformat;
use crate::sinks::{BoundedSink, CountingSink};
use crate::traits::{FloatFmt, FmtEnv};
use crate::types::{FmtErr, FmtOptions};

const INT_MAX: usize = i32::MAX as usize;

/// Format into a bounded byte buffer.
///
/// `cap` is the caller's `n` (`size_t`); `out` must be able to hold `cap` bytes, but a
/// shorter slice is handled by storing only into it.
///
/// # Errors
/// [`FmtErr::Overflow`] when `cap - 1` exceeds `INT_MAX` or the result does, and any error
/// of [`crate::vformat`].
pub fn vsnprintf_core<A: FmtEnv, F: FloatFmt>(
    out: &mut [u8],
    cap: usize,
    fmt: &[u8],
    args: &mut A,
    opts: &FmtOptions<'_>,
    floats: &F,
) -> Result<i32, FmtErr> {
    if cap == 0 {
        let mut sink = CountingSink::<u8>::new();
        return vformat(&mut sink, fmt, args, opts, floats);
    }
    let limit = cap.saturating_sub(1);
    if limit > INT_MAX {
        if let Some(slot) = out.get_mut(0) {
            *slot = 0;
        }
        return Err(FmtErr::Overflow);
    }
    let store = core::cmp::min(limit, out.len().saturating_sub(1));
    let (result, total, stored) = {
        let mut sink = BoundedSink::new(out, store);
        let result = vformat(&mut sink, fmt, args, opts, floats);
        (result, sink.len(), sink.stored())
    };
    if let Some(slot) = out.get_mut(stored) {
        *slot = 0;
    }
    match result {
        Ok(_) if total > INT_MAX => Err(FmtErr::Overflow),
        Ok(_) => Ok(total as i32),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "snprintf_ut.rs"]
mod tests;
