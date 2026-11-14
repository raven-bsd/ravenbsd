// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_setcancelstate(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::cancel;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_setcancelstate(state: c_int, old: *mut c_int) -> c_int {
    match cancel::setcancelstate(&SysKernel, state) {
        Ok(previous) => {
            if !old.is_null() {
                unsafe { *old = previous };
            }
            0
        }
        Err(e) => super::common::err(e),
    }
}
