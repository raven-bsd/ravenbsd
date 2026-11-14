// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_mutex_getprioceiling(3)`.
use core::ffi::c_int;
use raven_libc_thread::mutex as m;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_mutex_getprioceiling(mutex: *const m::Mutex, out: *mut c_int) -> c_int {
    if mutex.is_null() || out.is_null() {
        return 22;
    }
    let _ = mutex;
    let _ = out;
    61
}
