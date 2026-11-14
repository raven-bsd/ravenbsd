// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tss_set(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::c11;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::once::Once;
#[unsafe(no_mangle)]
pub extern "C" fn tss_set(key: u32, value: *mut c_void) -> c_int {
    c11::tss_set(&SysKernel, key, value as usize)
}
