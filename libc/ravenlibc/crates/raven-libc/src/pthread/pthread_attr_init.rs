// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_attr_init(3)`.
use raven_libc_thread::thread::attr as a;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_attr_init(attr: *mut a::Attr) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    // SAFETY: the caller supplies a writable pthread_attr_t.
    unsafe { *attr = a::init() };
    0
}
