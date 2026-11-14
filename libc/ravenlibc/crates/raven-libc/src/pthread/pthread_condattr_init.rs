// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_condattr_init(3)`.
use raven_libc_thread::cond as c;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_condattr_init(attr: *mut c::CondAttr) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    unsafe { *attr = c::attr_init() }; // SAFETY: attr is non-NULL after the check and writable for one condition attribute.
    0
}
