// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_rwlockattr_init(3)`.
use raven_libc_thread::rwlock as r;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_rwlockattr_init(attr: *mut r::RwLockAttr) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    unsafe { *attr = r::attr_init() };
    0
}
