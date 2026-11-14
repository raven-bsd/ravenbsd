// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_getaffinity_np(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::sched;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_getaffinity_np(
    _thread: usize,
    size: usize,
    mask: *mut core::ffi::c_void,
) -> c_int {
    if mask.is_null() || size < 8 {
        return 22;
    }
    match sched::getaffinity() {
        Ok(value) => {
            unsafe { *(mask as *mut u64) = value };
            0
        }
        Err(e) => super::common::err(e),
    }
}
