// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dup(2)`.
use core::ffi::c_int;

/// Duplicate `fd` at the lowest free descriptor.
#[unsafe(no_mangle)]
pub extern "C" fn dup(fd: c_int) -> c_int {
    match crate::ffi::platform::global().dup(fd, 0, false) {
        Ok(value) => value,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
