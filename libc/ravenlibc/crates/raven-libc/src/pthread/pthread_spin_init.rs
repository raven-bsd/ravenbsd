// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_spin_init(3)`.
use core::ffi::c_int;
use raven_libc_thread::spin as s;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_spin_init(lock: *mut s::SpinLock, shared: c_int) -> c_int {
    if lock.is_null() {
        return 22;
    }
    // SAFETY: lock is non-NULL after the check and exclusively borrowed for initialisation.
    match s::init(unsafe { &mut *lock }, shared) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
