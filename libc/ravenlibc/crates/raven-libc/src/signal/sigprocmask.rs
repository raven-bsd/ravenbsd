// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigprocmask(3)`: the authoritative C19 kernel mask with the libc mirror kept in step.
use core::ffi::c_int;

use raven_libc_abi::errno::Errno;
use raven_libc_signal::kernel::SysKernel;
use raven_libc_signal::{SigSet, maskops};

/// Change the calling thread's signal mask.
///
/// # Safety
/// `set`/`old` are NULL or point to `sigset_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigprocmask(how: c_int, set: *const SigSet, old: *mut SigSet) -> c_int {
    // SAFETY: the caller guarantees the non-NULL pointers name live sigset_t objects.
    let set = if set.is_null() {
        None
    } else {
        Some(unsafe { &*set })
    };
    // SAFETY: as above; `old` is writable when non-NULL.
    let old = if old.is_null() {
        None
    } else {
        Some(unsafe { &mut *old })
    };
    let result = maskops::pthread_sigmask(&SysKernel, how, set, old);
    if result != 0 {
        crate::ffi::errno::set(Errno(result.unsigned_abs() as u16));
        return -1;
    }
    0
}
