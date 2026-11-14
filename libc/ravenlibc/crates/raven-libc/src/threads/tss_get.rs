// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tss_get(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::c11;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::once::Once;
#[unsafe(no_mangle)]
pub extern "C" fn tss_get(key: u32) -> *mut c_void {
    c11::tss_get(&SysKernel, key) as *mut c_void
}
