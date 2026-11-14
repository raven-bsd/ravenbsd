// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `__h_errno_location(3)`.
use core::ffi::c_int;

/// The address of the `h_errno` cell.
#[unsafe(no_mangle)]
pub extern "C" fn __h_errno_location() -> *mut c_int {
    &raw mut crate::netdb::h_errno::h_errno
}
