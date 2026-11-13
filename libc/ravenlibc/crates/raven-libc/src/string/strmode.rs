// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strmode(3)`.
use core::ffi::c_char;

/// The `ls -l` mode string for `mode` into `p` (12 bytes).
///
/// # Safety
/// `p` has 12 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strmode(mode: u32, p: *mut c_char) {
    // SAFETY: the caller's contract is the strmode(3) contract.
    unsafe { raven_libc_str::raw::strmode(mode, p.cast()) };
}
