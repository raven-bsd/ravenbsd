// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_attr_setdetachstate(3)`.
use raven_libc_thread::thread::attr as a;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_attr_setdetachstate(
    attr: *mut a::Attr,
    state: core::ffi::c_int,
) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    // SAFETY: attr is non-NULL after the check and exclusively borrowed for the call.
    match a::setdetachstate(unsafe { &mut *attr }, state) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
