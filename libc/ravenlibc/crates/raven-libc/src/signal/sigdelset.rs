// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigdelset(3)`.
use core::ffi::c_int;

use raven_libc_signal::SigSet;

/// del `sig` to/from `set`.
///
/// # Safety
/// `set` points to a writable `sigset_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigdelset(set: *mut SigSet, sig: c_int) -> c_int {
    if set.is_null() {
        return -1;
    }
    // SAFETY: the caller supplies a writable sigset_t.
    match unsafe { (*set).del(sig) } {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
