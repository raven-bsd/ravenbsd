// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strcat(3)` and `strncat(3)`.
use core::ffi::c_char;

/// Append `src` to the NUL-terminated `dst`.
///
/// # Safety
/// `dst` has room for its current length, `strlen(src)`, and a NUL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcat(dst: *mut c_char, src: *const c_char) -> *mut c_char {
    // SAFETY: the caller's contract is the strcat(3) contract.
    unsafe { raven_libc_str::raw::strcat(dst.cast(), src.cast()).cast() }
}

/// Append at most `n` bytes of `src`.
///
/// # Safety
/// `dst` has room for its current length, up to `n` copied bytes, and a NUL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncat(dst: *mut c_char, src: *const c_char, n: usize) -> *mut c_char {
    // SAFETY: the caller's contract is the strncat(3) contract.
    unsafe { raven_libc_str::raw::strncat(dst.cast(), src.cast(), n).cast() }
}
