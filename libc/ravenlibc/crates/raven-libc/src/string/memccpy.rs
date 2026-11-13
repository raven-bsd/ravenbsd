// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `memccpy(3)`.
use core::ffi::{c_int, c_void};

/// Copy until `c` is stored or `n` bytes are copied.
///
/// # Safety
/// `dest` has `n` writable bytes and `src` `n` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memccpy(
    dest: *mut c_void,
    src: *const c_void,
    c: c_int,
    n: usize,
) -> *mut c_void {
    // SAFETY: the caller's contract is the memccpy(3) contract.
    unsafe { raven_libc_str::raw::memccpy(dest.cast(), src.cast(), c, n).cast() }
}
