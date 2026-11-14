// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_cancel(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::cancel;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_cancel(thread: usize) -> c_int {
    match cancel::cancel(&SysKernel, thread) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
