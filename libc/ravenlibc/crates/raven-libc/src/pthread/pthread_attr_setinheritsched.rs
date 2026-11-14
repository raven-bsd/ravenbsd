// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_attr_setinheritsched(3)`.
use raven_libc_thread::thread::attr as a;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_attr_setinheritsched(
    attr: *mut a::Attr,
    inherit: core::ffi::c_int,
) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    match a::setinheritsched(unsafe { &mut *attr }, inherit) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
