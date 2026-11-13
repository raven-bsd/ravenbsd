// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `lseek(2)`.
use core::ffi::c_int;

/// Reposition `fd` and return the new offset.
#[unsafe(no_mangle)]
pub extern "C" fn lseek(fd: c_int, offset: i64, whence: c_int) -> i64 {
    match crate::ffi::platform::global().lseek(fd, offset, whence) {
        Ok(value) => value,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
