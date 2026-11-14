// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_mutex_clocklock(3)`.
use core::ffi::c_int;
use raven_libc_thread::mutex as m;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_mutex_clocklock(
    mutex: *mut m::Mutex,
    _clock: c_int,
    at: *const raven_libc_abi::structs::Timespec,
) -> c_int {
    if mutex.is_null() {
        return 22;
    }
    match m::timedlock(
        &raven_libc_thread::raw::SysKernel,
        unsafe { &*mutex }, // SAFETY: mutex is non-NULL after the check and points to a live initialised mutex.
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
