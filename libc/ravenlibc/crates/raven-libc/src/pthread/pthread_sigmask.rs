// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_sigmask(3)`.
use core::ffi::c_int;
use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::{SigSet, maskops};
#[unsafe(no_mangle)]
pub extern "C" fn pthread_sigmask(how: c_int, set: *const SigSet, old: *mut SigSet) -> c_int {
    // SIG_SETUP is per thread: the first signal operation on a secondary thread registers that
    // thread (and its own context area) before an unblock can kick it.
    if let Err(error) = crate::signal::ensure_entry() {
        crate::ffi::errno::set(error);
        return -1;
    }
    let s = if set.is_null() {
        None
    } else {
        Some(unsafe { &*set }) // SAFETY: set is non-NULL when Some; the caller provides a valid sigset_t.
    };
    let o = if old.is_null() {
        None
    } else {
        Some(unsafe { &mut *old }) // SAFETY: old is non-NULL when Some and writable for one sigset_t.
    };
    maskops::pthread_sigmask(&SysKernel, how, s, o)
}
