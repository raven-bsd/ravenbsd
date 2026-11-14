// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_attr_setschedpolicy(3)`.
use raven_libc_thread::thread::attr as a;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_attr_setschedpolicy(
    attr: *mut a::Attr,
    policy: core::ffi::c_int,
) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    match a::setschedpolicy(
        &raven_libc_thread::raw::SysKernel,
        unsafe { &mut *attr }, // SAFETY: attr is non-NULL after the check and exclusively borrowed for the call.
        policy,
    ) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
