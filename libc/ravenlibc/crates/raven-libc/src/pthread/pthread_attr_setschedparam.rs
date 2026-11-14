// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_attr_setschedparam(3)`.
use raven_libc_thread::thread::attr as a;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_attr_setschedparam(
    attr: *mut a::Attr,
    param: *const core::ffi::c_int,
) -> core::ffi::c_int {
    if attr.is_null() || param.is_null() {
        return 22;
    }
    a::setschedparam(unsafe { &mut *attr }, unsafe { *param });
    0
}
