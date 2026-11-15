// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ftruncate(2)`.
use core::ffi::{c_int, c_longlong};

/// Set the length of the open `fd`.
#[unsafe(no_mangle)]
pub extern "C" fn ftruncate(fd: c_int, length: c_longlong) -> c_int {
    crate::ffi::ret::int(
        crate::ffi::platform::global()
            .ftruncate(fd, length)
            .map(|()| 0)
            .map_err(crate::platform::map_error),
    )
}
