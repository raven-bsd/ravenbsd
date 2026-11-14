// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_mutexattr_settype(3)`.
use raven_libc_thread::mutex as m;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_mutexattr_settype(
    attr: *mut m::MutexAttr,
    value: core::ffi::c_int,
) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    // SAFETY: attr is non-NULL after the check and exclusively borrowed for the call.
    match m::attr_settype(unsafe { &mut *attr }, value) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
