// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `thrd_yield(3)`.
use core::ffi::{c_int, c_void};
use raven_libc_thread::c11;
use raven_libc_thread::raw::SysKernel;
#[unsafe(no_mangle)]
pub extern "C" fn thrd_yield() {
    c11::thrd_yield(&SysKernel);
}
