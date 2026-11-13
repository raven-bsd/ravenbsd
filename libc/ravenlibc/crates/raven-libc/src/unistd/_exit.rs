// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `_exit(2)`.
use core::ffi::c_int;

/// Terminate the process immediately.
#[unsafe(no_mangle)]
pub extern "C" fn _exit(status: c_int) -> ! {
    raven_libc_sys::sys::exit(status)
}
