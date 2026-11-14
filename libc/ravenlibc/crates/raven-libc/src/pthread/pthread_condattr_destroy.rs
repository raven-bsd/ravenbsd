// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_condattr_destroy(3)`.
use raven_libc_thread::cond as c;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_condattr_destroy(attr: *mut c::CondAttr) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    c::attr_destroy(unsafe { &*attr });
    0
}
