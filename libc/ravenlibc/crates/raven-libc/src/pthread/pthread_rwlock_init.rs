// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_rwlock_init(3)`.
use core::ffi::c_int;
use raven_libc_thread::rwlock as r;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_rwlock_init(lock: *mut r::RwLock, attr: *const r::RwLockAttr) -> c_int {
    if lock.is_null() {
        return 22;
    }
    let a = if attr.is_null() {
        None
    } else {
        Some(unsafe { &*attr })
    };
    r::init(unsafe { &mut *lock }, a);
    0
}
