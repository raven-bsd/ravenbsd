// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fchmod(2)`.
use core::ffi::{c_int, c_uint};

/// Change the mode of the open `fd`.
#[unsafe(no_mangle)]
pub extern "C" fn fchmod(fd: c_int, mode: c_uint) -> c_int {
    crate::ffi::ret::int(
        crate::ffi::platform::global()
            .fchmod(fd, mode)
            .map(|()| 0)
            .map_err(crate::platform::map_error),
    )
}
