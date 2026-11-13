// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `memmove(3)`.
use core::ffi::c_void;

/// Copy `n` bytes, tolerating overlap.
///
/// # Safety
/// `dest` has `n` writable bytes and `src` `n` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    // SAFETY: the caller's contract is the memmove(3) contract.
    unsafe { raven_libc_str::raw::memmove(dest.cast(), src.cast(), n).cast() }
}
