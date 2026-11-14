// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `cnd_wait(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::{c11, cond::Cond, mutex::Mutex};
#[unsafe(no_mangle)]
pub extern "C" fn cnd_wait(cond: *mut Cond, mutex: *mut Mutex) -> c_int {
    if cond.is_null() || mutex.is_null() {
        return 2;
    }
    c11::cnd_wait(&SysKernel, unsafe { &*cond }, unsafe { &*mutex })
}
