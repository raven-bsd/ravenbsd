// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_attr_setstack(3)`.
use raven_libc_thread::thread::attr as a;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_attr_setstack(
    attr: *mut a::Attr,
    addr: *mut core::ffi::c_void,
    size: usize,
) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    match a::setstack(unsafe { &mut *attr }, addr as usize, size) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
