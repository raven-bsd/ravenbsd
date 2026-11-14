// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `__error(3)`: the FreeBSD spelling of the errno cell address.
use core::ffi::c_int;

/// The calling thread's errno cell.
#[unsafe(no_mangle)]
pub extern "C" fn __error() -> *mut c_int {
    crate::ffi::errno::ptr()
}
