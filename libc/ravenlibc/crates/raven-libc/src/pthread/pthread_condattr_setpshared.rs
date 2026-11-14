// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_condattr_setpshared(3)`.
use raven_libc_thread::cond as c;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_condattr_setpshared(
    attr: *mut c::CondAttr,
    value: core::ffi::c_int,
) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    match c::attr_setpshared(
        &raven_libc_thread::raw::SysKernel,
        unsafe { &mut *attr },
        value,
    ) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
