// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_setconcurrency(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::sched;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_setconcurrency(value: c_int) -> c_int {
    match sched::setconcurrency(value) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
