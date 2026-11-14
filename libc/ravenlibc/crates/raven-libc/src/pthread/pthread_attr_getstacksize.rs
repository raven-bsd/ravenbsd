// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_attr_getstacksize(3)`.
use raven_libc_thread::thread::attr as a;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_attr_getstacksize(
    attr: *const a::Attr,
    out: *mut usize,
) -> core::ffi::c_int {
    if attr.is_null() || out.is_null() {
        return 22;
    }
    unsafe { *out = a::getstacksize(&*attr) };
    0
}
