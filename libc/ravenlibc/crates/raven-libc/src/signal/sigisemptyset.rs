// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigisemptyset(3)`.
use core::ffi::c_int;

use raven_libc_signal::SigSet;

/// True when `set` has no members.
///
/// # Safety
/// `set` points to a readable `sigset_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigisemptyset(set: *const SigSet) -> c_int {
    if set.is_null() {
        return -1;
    }
    // SAFETY: the caller supplies a readable sigset_t.
    i32::from(unsafe { (*set).is_empty() })
}
