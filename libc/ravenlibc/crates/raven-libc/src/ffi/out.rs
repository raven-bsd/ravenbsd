// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Copying a result back into a caller's buffer with NUL termination inside `cap`.

use core::ffi::c_char;

/// Copy `bytes` and a NUL into `dst`, never writing more than `cap` bytes.
///
/// Returns the number of bytes copied without the NUL; `cap == 0` writes nothing.
///
/// # Safety
/// `dst` is NULL or points to `cap` writable bytes.
pub(crate) unsafe fn write_cstr(dst: *mut c_char, bytes: &[u8], cap: usize) -> usize {
    if dst.is_null() || cap == 0 {
        return 0;
    }
    let room = cap.saturating_sub(1);
    let copied = core::cmp::min(room, bytes.len());
    // SAFETY: the caller granted `cap` writable bytes and `copied <= cap - 1`.
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), dst.cast::<u8>(), copied);
        *dst.cast::<u8>().add(copied) = 0;
    }
    copied
}

/// Copy raw bytes without a terminator, bounded by `cap`.
///
/// # Safety
/// `dst` is NULL or points to `cap` writable bytes.
pub(crate) unsafe fn write_bytes(dst: *mut u8, bytes: &[u8], cap: usize) -> usize {
    if dst.is_null() {
        return 0;
    }
    let copied = core::cmp::min(cap, bytes.len());
    // SAFETY: the caller granted `cap` writable bytes and `copied <= cap`.
    unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, copied) };
    copied
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "out_ut.rs"]
mod tests;
