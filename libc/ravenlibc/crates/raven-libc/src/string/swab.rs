// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `swab(3)`.
use core::ffi::c_char;

/// Swap adjacent bytes of `from` into `to`.
///
/// # Safety
/// `from` has `len` readable bytes and `to` has `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn swab(from: *const c_char, to: *mut c_char, len: isize) {
    // SAFETY: the caller's contract is the swab(3) contract.
    unsafe { raven_libc_str::raw::swab(from.cast(), to.cast(), len) };
}
