// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sem_clockwait(3)`.
use core::ffi::c_int;
use raven_libc_thread::sem as s;
#[unsafe(no_mangle)]
pub extern "C" fn sem_clockwait(
    sem: *mut s::Sem,
    _clock: c_int,
    at: *const raven_libc_abi::structs::Timespec,
) -> c_int {
    if sem.is_null() {
        return 22;
    }
    match s::clockwait(
        &raven_libc_thread::raw::SysKernel,
        unsafe { &*sem },
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
