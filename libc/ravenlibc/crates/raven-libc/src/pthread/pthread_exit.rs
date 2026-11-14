// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_exit(3)`.
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread;

/// Exit the calling thread with `result`.
#[unsafe(no_mangle)]
pub extern "C" fn pthread_exit(result: *mut core::ffi::c_void) -> ! {
    thread::exit(&SysKernel, result as usize)
}
