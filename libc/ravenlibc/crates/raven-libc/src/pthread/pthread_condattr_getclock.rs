// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_condattr_getclock(3)`.
use raven_libc_thread::cond as c;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_condattr_getclock(
    attr: *const c::CondAttr,
    out: *mut core::ffi::c_int,
) -> core::ffi::c_int {
    if attr.is_null() || out.is_null() {
        return 22;
    }
    unsafe { *out = c::attr_getclock(&*attr) };
    0
}
