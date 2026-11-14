// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_rwlock_unlock(3)`.
use core::ffi::c_int;
use raven_libc_thread::rwlock as r;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_rwlock_unlock(lock: *mut r::RwLock) -> c_int {
    if lock.is_null() {
        return 22;
    }
    match r::unlock(&raven_libc_thread::raw::SysKernel, unsafe { &*lock }) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
