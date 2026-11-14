// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_mutexattr_getprioceiling(3)`.
use raven_libc_thread::mutex as m;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_mutexattr_getprioceiling(
    attr: *const m::MutexAttr,
    out: *mut core::ffi::c_int,
) -> core::ffi::c_int {
    if attr.is_null() || out.is_null() {
        return 22;
    }
    let _ = attr;
    let _ = out;
    61
}
