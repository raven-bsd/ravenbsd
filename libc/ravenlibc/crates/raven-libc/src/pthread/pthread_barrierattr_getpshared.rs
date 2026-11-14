// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_barrierattr_getpshared(3)`.
use raven_libc_thread::barrier as b;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_barrierattr_getpshared(
    attr: *const b::BarrierAttr,
    out: *mut core::ffi::c_int,
) -> core::ffi::c_int {
    if attr.is_null() || out.is_null() {
        return 22;
    }
    unsafe { *out = b::attr_getpshared(&*attr) };
    0
}
