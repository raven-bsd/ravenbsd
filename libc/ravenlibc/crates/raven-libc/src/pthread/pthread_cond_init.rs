// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_cond_init(3)`.
use core::ffi::c_int;
use raven_libc_thread::cond as c;
use raven_libc_thread::mutex as m;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_cond_init(cond: *mut c::Cond, attr: *const c::CondAttr) -> c_int {
    if cond.is_null() {
        return 22;
    }
    let a = if attr.is_null() {
        None
    } else {
        Some(unsafe { &*attr })
    };
    c::init(unsafe { &mut *cond }, a);
    0
}
