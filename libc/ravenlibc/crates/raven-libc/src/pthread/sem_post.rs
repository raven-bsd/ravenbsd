// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sem_post(3)`.
use core::ffi::c_int;
use raven_libc_thread::sem as s;
#[unsafe(no_mangle)]
pub extern "C" fn sem_post(sem: *mut s::Sem) -> c_int {
    if sem.is_null() {
        return 22;
    }
    // SAFETY: sem is non-NULL after the check and points to a live initialised semaphore.
    match s::post(&raven_libc_thread::raw::SysKernel, unsafe { &*sem }) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
