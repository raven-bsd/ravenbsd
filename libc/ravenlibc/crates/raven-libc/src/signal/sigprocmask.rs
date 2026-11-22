// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigprocmask(3)`.
use core::ffi::c_int;

use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::{SigSet, maskops};

/// Change the calling thread's signal mask.
///
/// # Safety
/// `set`/`old` are NULL or point to `sigset_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigprocmask(how: c_int, set: *const SigSet, old: *mut SigSet) -> c_int {
    if let Err(error) = super::ensure_entry() {
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
    match maskops::sigprocmask(&SysKernel, how, s, o) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
