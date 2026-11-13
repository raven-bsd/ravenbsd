// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `memcpy(3)`.
use core::ffi::c_void;

/// Copy `n` bytes; the regions must not overlap.
///
/// # Safety
/// `dest` has `n` writable bytes and `src` `n` readable bytes; the regions do not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    // SAFETY: the caller's contract is the memcpy(3) contract.
    unsafe { raven_libc_str::raw::memcpy(dest.cast(), src.cast(), n).cast() }
}
