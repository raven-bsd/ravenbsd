// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_kill(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::kill;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_kill(thread: usize, signal: c_int) -> c_int {
    match kill::kill(&SysKernel, thread, signal) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
