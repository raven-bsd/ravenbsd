// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tss_delete(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::c11;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::once::Once;
#[unsafe(no_mangle)]
pub extern "C" fn tss_delete(key: u32) {
    c11::tss_delete(&SysKernel, key);
}
