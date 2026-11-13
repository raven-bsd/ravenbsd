// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `memset(3)`.
use core::ffi::{c_int, c_void};

/// Fill `n` bytes with `c`.
///
/// # Safety
/// `s` has `n` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void {
    // SAFETY: the caller's contract is the memset(3) contract.
    unsafe { raven_libc_str::raw::memset(s.cast(), c, n).cast() }
}
