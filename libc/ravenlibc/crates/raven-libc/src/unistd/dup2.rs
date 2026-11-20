// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dup2(2)`.
use core::ffi::c_int;

/// Duplicate `old` onto `new`.
#[unsafe(no_mangle)]
pub extern "C" fn dup2(old: c_int, new: c_int) -> c_int {
    match crate::ffi::platform::global().dup2(old, new, false) {
        Ok(value) => value,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
