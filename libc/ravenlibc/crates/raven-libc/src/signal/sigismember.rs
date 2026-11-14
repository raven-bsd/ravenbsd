// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigismember(3)`.
use core::ffi::c_int;

use raven_libc_signal::SigSet;

/// Test membership.
///
/// # Safety
/// `set` points to a readable `sigset_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigismember(set: *const SigSet, sig: c_int) -> c_int {
    if set.is_null() {
        return -1;
    }
    // SAFETY: the caller supplies a readable sigset_t.
    match unsafe { (*set).is_member(sig) } {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
