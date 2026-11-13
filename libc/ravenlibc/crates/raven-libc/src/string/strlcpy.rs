// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strlcpy(3)` and `strlcat(3)`.
use core::ffi::c_char;

/// Copy `src` into `dst` of size `size`, always NUL terminating.
///
/// # Safety
/// `dst` has `size` writable bytes and `src` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strlcpy(dst: *mut c_char, src: *const c_char, size: usize) -> usize {
    // SAFETY: the caller's contract is the strlcpy(3) contract.
    unsafe { raven_libc_str::raw::strlcpy(dst.cast(), src.cast(), size) }
}

/// Append `src` to `dst` of size `size`, always NUL terminating.
///
/// # Safety
/// `dst` has `size` writable bytes and both strings are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strlcat(dst: *mut c_char, src: *const c_char, size: usize) -> usize {
    // SAFETY: the caller's contract is the strlcat(3) contract.
    unsafe { raven_libc_str::raw::strlcat(dst.cast(), src.cast(), size) }
}
