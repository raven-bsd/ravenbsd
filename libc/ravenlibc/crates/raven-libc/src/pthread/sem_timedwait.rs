// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sem_timedwait(3)`.
use core::ffi::c_int;
use raven_libc_thread::sem as s;
#[unsafe(no_mangle)]
pub extern "C" fn sem_timedwait(
    sem: *mut s::Sem,
    at: *const raven_libc_abi::structs::Timespec,
) -> c_int {
    if sem.is_null() {
        return super::common::sem_err(raven_libc_thread::limits::EINVAL);
    }
    match s::timedwait(
        &raven_libc_thread::raw::SysKernel,
        unsafe { &*sem }, // SAFETY: sem is non-NULL after the check and points to a live initialised semaphore.
        if at.is_null() {
            None
        } else {
            Some(unsafe { *at }) // SAFETY: at is non-NULL when Some and readable for one timespec.
        },
    ) {
        Ok(()) => 0,
        Err(e) => super::common::sem_err(e),
    }
}
