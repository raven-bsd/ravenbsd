// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_rwlockattr_getpshared(3)`.
use raven_libc_thread::rwlock as r;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_rwlockattr_getpshared(
    attr: *const r::RwLockAttr,
    out: *mut core::ffi::c_int,
) -> core::ffi::c_int {
    if attr.is_null() || out.is_null() {
        return 22;
    }
    unsafe { *out = r::attr_getpshared(&*attr) }; // SAFETY: the null checks above guarantee a readable attribute and a writable out slot.
    0
}
