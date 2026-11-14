// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_barrierattr_destroy(3)`.
use raven_libc_thread::barrier as b;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_barrierattr_destroy(attr: *mut b::BarrierAttr) -> core::ffi::c_int {
    if attr.is_null() {
        return 22;
    }
    b::attr_destroy(unsafe { &*attr }); // SAFETY: attr is non-NULL after the check and points to a live barrier attribute.
    0
}
