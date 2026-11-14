// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_cond_clockwait(3)`.
use core::ffi::c_int;
use raven_libc_thread::cond as c;
use raven_libc_thread::mutex as m;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_cond_clockwait(
    cond: *mut c::Cond,
    mutex: *mut m::Mutex,
    clock: c_int,
    at: *const raven_libc_abi::structs::Timespec,
) -> c_int {
    if cond.is_null() || mutex.is_null() {
        return 22;
    }
    match c::clockwait(
        &raven_libc_thread::raw::SysKernel,
        unsafe { &*cond },
        unsafe { &*mutex },
        clock as u32,
        if at.is_null() {
            None
        } else {
            Some(unsafe { *at })
        },
    ) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
