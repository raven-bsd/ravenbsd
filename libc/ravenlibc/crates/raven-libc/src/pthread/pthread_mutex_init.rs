// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_mutex_init(3)`.
use core::ffi::c_int;
use raven_libc_thread::mutex as m;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_mutex_init(mutex: *mut m::Mutex, attr: *const m::MutexAttr) -> c_int {
    if mutex.is_null() {
        return 22;
    }
    let a = if attr.is_null() {
        None
    } else {
        Some(unsafe { &*attr })
    };
    m::init(unsafe { &mut *mutex }, a);
    0
}
