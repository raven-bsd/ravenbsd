// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_setschedparam(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::sched;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_setschedparam(
    thread: usize,
    policy: c_int,
    param: *const c_int,
) -> c_int {
    if param.is_null() {
        return 22;
    }
    match sched::setschedparam(&SysKernel, thread, policy, unsafe { *param }) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
