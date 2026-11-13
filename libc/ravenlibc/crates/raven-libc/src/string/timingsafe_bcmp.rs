// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `timingsafe_bcmp(3)` and `timingsafe_memcmp(3)`.
use core::ffi::c_void;

/// Constant-time equality: zero when equal.
///
/// # Safety
/// Both pointers grant `n` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timingsafe_bcmp(a: *const c_void, b: *const c_void, n: usize) -> i32 {
    // SAFETY: the caller grants two `n`-byte readable regions.
    let left = unsafe { core::slice::from_raw_parts(a.cast::<u8>(), n) };
    // SAFETY: as above.
    let right = unsafe { core::slice::from_raw_parts(b.cast::<u8>(), n) };
    raven_libc_str::secure::timingsafe_bcmp(left, right)
}

/// Constant-time comparison: the sign of the difference as in `memcmp`.
///
/// # Safety
/// Both pointers grant `n` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timingsafe_memcmp(a: *const c_void, b: *const c_void, n: usize) -> i32 {
    // SAFETY: the caller grants two `n`-byte readable regions.
    let left = unsafe { core::slice::from_raw_parts(a.cast::<u8>(), n) };
    // SAFETY: as above.
    let right = unsafe { core::slice::from_raw_parts(b.cast::<u8>(), n) };
    raven_libc_str::secure::timingsafe_memcmp(left, right)
}
