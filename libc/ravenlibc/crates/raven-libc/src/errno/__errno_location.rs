// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `__errno_location(3)`.
use core::ffi::c_int;

/// The address of the calling thread's `errno` cell.
#[unsafe(no_mangle)]
pub extern "C" fn __errno_location() -> *mut c_int {
    crate::ffi::errno::ptr()
}
