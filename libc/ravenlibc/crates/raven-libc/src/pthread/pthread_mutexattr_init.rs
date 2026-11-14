// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_mutexattr_init(3)`.
use raven_libc_thread::mutex as m;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_mutexattr_init(attr: *mut m::MutexAttr) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    unsafe { *attr = m::attr_init() }; // SAFETY: attr is non-NULL after the check and writable for one mutex attribute.
    0
}
