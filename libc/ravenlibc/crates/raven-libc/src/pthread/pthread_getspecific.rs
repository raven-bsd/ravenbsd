// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_getspecific(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::tsd;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_getspecific(key: u32) -> *mut c_void {
    tsd::getspecific(&SysKernel, key) as *mut c_void
}
