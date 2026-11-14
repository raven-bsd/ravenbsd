// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_cond_signal(3)`.
use core::ffi::c_int;
use raven_libc_thread::cond as c;
use raven_libc_thread::mutex as m;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_cond_signal(cond: *mut c::Cond) -> c_int {
    if cond.is_null() {
        return 22;
    }
    // SAFETY: cond is non-NULL after the check and points to a live initialised condition variable.
    match c::signal(&raven_libc_thread::raw::SysKernel, unsafe { &*cond }) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
