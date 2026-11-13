// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `close(2)`.
use core::ffi::c_int;

/// Close `fd`.
#[unsafe(no_mangle)]
pub extern "C" fn close(fd: c_int) -> c_int {
    match crate::ffi::platform::global().close(fd) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
