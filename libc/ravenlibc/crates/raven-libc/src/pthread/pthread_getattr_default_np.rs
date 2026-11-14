// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_getattr_default_np(3)`.
use raven_libc_thread::thread::attr as a;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_getattr_default_np(attr: *mut a::Attr) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    unsafe { *attr = a::getattr_default_np() };
    0
}
