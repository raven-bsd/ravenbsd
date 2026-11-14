// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `lseek64`: the 64-bit `off_t` spelling of `lseek` (ticket C66 5.F).
use core::ffi::c_int;

/// Forward to `lseek`.
#[unsafe(no_mangle)]
pub extern "C" fn lseek64(fd: c_int, offset: i64, whence: c_int) -> i64 {
    match crate::ffi::platform::global().lseek(fd, offset, whence) {
        Ok(value) => value,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
