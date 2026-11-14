// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_getcpuclockid(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::{self, attr as a};
#[unsafe(no_mangle)]
pub extern "C" fn pthread_getcpuclockid(thread: usize, out: *mut c_int) -> c_int {
    if out.is_null() {
        return 22;
    }
    let _ = thread;
    61
}
