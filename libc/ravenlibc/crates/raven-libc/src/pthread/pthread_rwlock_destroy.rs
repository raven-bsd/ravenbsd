// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_rwlock_destroy(3)`.
use core::ffi::c_int;
use raven_libc_thread::rwlock as r;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_rwlock_destroy(lock: *mut r::RwLock) -> c_int {
    if lock.is_null() {
        return 22;
    }
    r::destroy(unsafe { &*lock });
    0
}
