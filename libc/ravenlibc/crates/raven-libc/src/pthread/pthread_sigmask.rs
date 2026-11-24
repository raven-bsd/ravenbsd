// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_sigmask(3)`.
use core::ffi::c_int;
use raven_libc_signal::SigSet;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_sigmask(how: c_int, set: *const SigSet, old: *mut SigSet) -> c_int {
    // SAFETY: set/old are NULL or point to a sigset_t, the same contract as sigprocmask.
    unsafe { crate::signal::sigprocmask(how, set, old) }
}
