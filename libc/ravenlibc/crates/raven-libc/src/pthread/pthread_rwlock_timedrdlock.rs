// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_rwlock_timedrdlock(3)`.
use core::ffi::c_int;
use raven_libc_thread::rwlock as r;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_rwlock_timedrdlock(
    lock: *mut r::RwLock,
    at: *const raven_libc_abi::structs::Timespec,
) -> c_int {
    if lock.is_null() {
        return 22;
    }
    match r::timedrdlock(
        &raven_libc_thread::raw::SysKernel,
        unsafe { &*lock },
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
