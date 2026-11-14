// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_mutex_unlock(3)`.
use core::ffi::c_int;
use raven_libc_thread::mutex as m;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_mutex_unlock(mutex: *mut m::Mutex) -> c_int {
    if mutex.is_null() {
        return 22;
    }
    // SAFETY: mutex is non-NULL after the check and points to a live locked mutex.
    match m::unlock(&raven_libc_thread::raw::SysKernel, unsafe { &*mutex }) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
