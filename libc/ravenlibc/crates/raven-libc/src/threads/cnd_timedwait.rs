// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `cnd_timedwait(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::{c11, cond::Cond, mutex::Mutex};
#[unsafe(no_mangle)]
pub extern "C" fn cnd_timedwait(
    cond: *mut Cond,
    mutex: *mut Mutex,
    at: *const raven_libc_abi::structs::Timespec,
) -> c_int {
    if cond.is_null() || mutex.is_null() || at.is_null() {
        return 2;
    }
    // SAFETY: the null checks above guarantee live cond/mutex objects and a readable timespec; the nested blocks dereference only those checked pointers.
    c11::cnd_timedwait(&SysKernel, unsafe { &*cond }, unsafe { &*mutex }, unsafe {
        *at
    })
}
