// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setpassent(3)`.
use core::ffi::c_int;

/// Rewind and report success.
#[unsafe(no_mangle)]
pub extern "C" fn setpassent(_stayopen: c_int) -> c_int {
    crate::pwd::setpwent::setpwent();
    1
}
