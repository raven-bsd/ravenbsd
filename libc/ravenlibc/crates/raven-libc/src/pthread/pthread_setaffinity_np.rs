// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_setaffinity_np(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::sched;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_setaffinity_np(
    thread: usize,
    size: usize,
    mask: *const core::ffi::c_void,
) -> c_int {
    if mask.is_null() || size < 8 {
        return 22;
    }
    // SAFETY: mask is non-NULL after the check and readable for one cpu_set_t word.
    match sched::setaffinity(thread, unsafe { *(mask as *const u64) }) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
