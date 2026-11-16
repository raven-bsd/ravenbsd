// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `__errno_location`.
use core::ffi::c_int;

/// The address of the process `errno` cell.
#[unsafe(no_mangle)]
pub extern "C" fn errno_location() -> *mut c_int {
    crate::ffi::errno::ptr()
}
