// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_detach(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::{self, attr as a};
#[unsafe(no_mangle)]
pub extern "C" fn pthread_detach(thread: usize) -> c_int {
    match thread::detach(&raven_libc_thread::raw::SysKernel, thread) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
