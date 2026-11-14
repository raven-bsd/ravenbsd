// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_cond_timedwait(3)`.
use core::ffi::c_int;
use raven_libc_thread::cond as c;
use raven_libc_thread::mutex as m;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_cond_timedwait(
    cond: *mut c::Cond,
    mutex: *mut m::Mutex,
    at: *const raven_libc_abi::structs::Timespec,
) -> c_int {
    if cond.is_null() || mutex.is_null() {
        return 22;
    }
    let clock = unsafe { (*cond).clock() }; // SAFETY: cond is non-NULL after the check and points to a live initialised condition variable.
    match c::timedwait(
        &raven_libc_thread::raw::SysKernel,
        unsafe { &*cond }, // SAFETY: cond is non-NULL after the check and points to a live initialised condition variable.
        unsafe { &*mutex }, // SAFETY: mutex is non-NULL after the check and points to a live locked mutex.
        clock,
        if at.is_null() {
            None
        } else {
            Some(unsafe { *at }) // SAFETY: at is non-NULL when Some and readable for one timespec.
        },
    ) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
