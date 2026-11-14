// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_testcancel(3)`.
use core::ffi::c_int;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::cancel;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_testcancel() {
    cancel::testcancel(&SysKernel);
}
