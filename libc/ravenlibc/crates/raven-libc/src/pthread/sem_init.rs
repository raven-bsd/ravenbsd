// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sem_init(3)`.
use core::ffi::c_int;
use raven_libc_thread::sem as s;
#[unsafe(no_mangle)]
pub extern "C" fn sem_init(sem: *mut s::Sem, pshared: c_int, value: u32) -> c_int {
    if sem.is_null() {
        return 22;
    }
    match s::init(
        &raven_libc_thread::raw::SysKernel,
        unsafe { &mut *sem }, // SAFETY: sem is non-NULL after the check and exclusively borrowed for initialisation.
        pshared,
        value,
    ) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
