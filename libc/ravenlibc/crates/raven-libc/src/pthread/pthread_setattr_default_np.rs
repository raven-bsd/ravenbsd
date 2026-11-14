// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_setattr_default_np(3)`.
use raven_libc_thread::thread::attr as a;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_setattr_default_np(attr: *const a::Attr) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    let attr_ref = unsafe { &*attr };
    match a::setattr_default_np(attr_ref) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
