// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `vswprintf` buffer semantics (CLEANROOM).
//!
//! Read for the contract only: `freebsd:lib/libc/stdio/vswprintf.c` and
//! `musl:src/stdio/vswprintf.c` (`donor-map/04` 5.2, R61). `cap == 0` is
//! [`FmtErr::Invalid`] (FreeBSD `EINVAL`; RD-new 3); `cap - 1 > INT_MAX` is
//! [`FmtErr::Overflow`] with `out[0] = 0`; output that does not fit in `cap - 1` wide
//! characters stores the first `cap - 1`, terminates, and is [`FmtErr::Overflow`].

use crate::printf::vformat;
use crate::sinks::BoundedSink;
use crate::traits::{FloatFmt, FmtEnv};
use crate::types::{FmtErr, FmtOptions};

const INT_MAX: usize = i32::MAX as usize;

/// Format into a bounded wide buffer.
///
/// # Errors
/// [`FmtErr::Invalid`] when `cap == 0`, [`FmtErr::Overflow`] on `INT_MAX` overflow or
/// truncation, [`FmtErr::Encoding`] for invalid wide input, and any error of
/// [`crate::vformat`].
pub fn vswprintf_core<A: FmtEnv, F: FloatFmt>(
    out: &mut [u32],
    cap: usize,
    fmt: &[u32],
    args: &mut A,
    opts: &FmtOptions<'_>,
    floats: &F,
) -> Result<i32, FmtErr> {
    if cap == 0 {
        return Err(FmtErr::Invalid);
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
        Ok(_) if total > limit => Err(FmtErr::Overflow),
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
#[path = "swprintf_ut.rs"]
mod tests;
