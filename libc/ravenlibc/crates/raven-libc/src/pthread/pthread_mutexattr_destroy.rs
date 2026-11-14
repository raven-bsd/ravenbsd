// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_mutexattr_destroy(3)`.
use raven_libc_thread::mutex as m;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_mutexattr_destroy(attr: *mut m::MutexAttr) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    m::attr_destroy(unsafe { &*attr }); // SAFETY: attr is non-NULL after the check and points to a live mutex attribute.
    0
}
