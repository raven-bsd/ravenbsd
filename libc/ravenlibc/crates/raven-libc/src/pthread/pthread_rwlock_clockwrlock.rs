// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_rwlock_clockwrlock(3)`.
use core::ffi::c_int;
use raven_libc_thread::rwlock as r;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_rwlock_clockwrlock(
    lock: *mut r::RwLock,
    _clock: c_int,
    at: *const raven_libc_abi::structs::Timespec,
) -> c_int {
    if lock.is_null() {
        return 22;
    }
    match r::timedwrlock(
        &raven_libc_thread::raw::SysKernel,
        unsafe { &*lock }, // SAFETY: lock is non-NULL after the check and points to a live initialised rwlock.
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
