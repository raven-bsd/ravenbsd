// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_barrierattr_init(3)`.
use raven_libc_thread::barrier as b;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_barrierattr_init(attr: *mut b::BarrierAttr) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    unsafe { *attr = b::attr_init() };
    0
}
