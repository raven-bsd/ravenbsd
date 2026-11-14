// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thrd_sleep(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::c11;
use raven_libc_thread::raw::SysKernel;
#[unsafe(no_mangle)]
pub extern "C" fn thrd_sleep(
    duration: *const raven_libc_abi::structs::Timespec,
    remaining: *mut raven_libc_abi::structs::Timespec,
) -> c_int {
    if duration.is_null() {
        return -1;
    }
    let duration = unsafe { *duration };
    let _ = remaining;
    c11::thrd_sleep(&SysKernel, duration, None)
}
