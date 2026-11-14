// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_spin_unlock(3)`.
use core::ffi::c_int;
use raven_libc_thread::spin as s;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_spin_unlock(lock: *mut s::SpinLock) -> c_int {
    if lock.is_null() {
        return 22;
    }
    s::unlock(unsafe { &*lock });
    0
}
