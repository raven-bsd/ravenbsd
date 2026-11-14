// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_spin_destroy(3)`.
use core::ffi::c_int;
use raven_libc_thread::spin as s;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_spin_destroy(lock: *mut s::SpinLock) -> c_int {
    if lock.is_null() {
        return 22;
    }
    s::destroy(unsafe { &*lock }); // SAFETY: lock is non-NULL after the check and points to a live initialised spinlock.
    0
}
