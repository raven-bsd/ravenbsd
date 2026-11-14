// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigfillset(3)`.
use core::ffi::c_int;

use raven_libc_signal::SigSet;

/// Fill `set` with every application signal.
///
/// # Safety
/// `set` points to a writable `sigset_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigfillset(set: *mut SigSet) -> c_int {
    if set.is_null() {
        return -1;
    }
    // SAFETY: the caller supplies a writable sigset_t.
    unsafe { (*set).fill() };
    0
}
