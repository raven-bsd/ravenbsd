// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_rwlockattr_destroy(3)`.
use raven_libc_thread::rwlock as r;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_rwlockattr_destroy(attr: *mut r::RwLockAttr) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    r::attr_destroy(unsafe { &*attr });
    0
}
