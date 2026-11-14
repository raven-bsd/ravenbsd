// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_getschedparam(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::sched;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_getschedparam(
    thread: usize,
    policy: *mut c_int,
    param: *mut c_int,
) -> c_int {
    match sched::getschedparam(&SysKernel, thread) {
        Ok((p, value)) => {
            if !policy.is_null() {
                unsafe { *policy = p };
            }
            if !param.is_null() {
                unsafe { *param = value };
            }
            0
        }
        Err(e) => super::common::err(e),
    }
}
