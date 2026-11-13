// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `memset_s(3)`, `explicit_bzero(3)` and `memset_explicit(3)`.
use core::ffi::{c_int, c_void};

/// The C11 Annex-K `memset_s`: zeroing the destination cannot be optimised away.
///
/// # Safety
/// `s` has `smax` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset_s(s: *mut c_void, smax: usize, c: c_int, n: usize) -> c_int {
    // SAFETY: the caller's contract is the memset_s(3) contract.
    unsafe { raven_libc_str::raw::memset_s(s.cast(), smax, c, n) }
}

/// Wipe `n` bytes without the compiler removing the stores.
///
/// # Safety
/// `p` has `n` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn explicit_bzero(p: *mut c_void, n: usize) {
    // SAFETY: the caller's contract is the explicit_bzero(3) contract.
    unsafe { raven_libc_str::raw::explicit_bzero(p.cast(), n) };
}

/// C23's name for `explicit_bzero`.
///
/// # Safety
/// `p` has `n` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset_explicit(p: *mut c_void, c: c_int, n: usize) {
    // SAFETY: the caller's contract is the memset_explicit(3) contract.
    unsafe { raven_libc_str::raw::memset_explicit(p.cast(), c, n) };
}
