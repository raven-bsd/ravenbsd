// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tcgetpgrp(3)`.
//!
//! The pty server stores the foreground group in its pair state and exposes the `GET_FG` custom
//! CALL op; the platform helper sends it.

use core::ffi::c_int;

/// Return the foreground process group of `fd`, or `-1` with the platform errno.
#[unsafe(no_mangle)]
pub extern "C" fn tcgetpgrp(fd: c_int) -> c_int {
    match raven_libc_platform::ioctl::tty::tcgetpgrp(crate::ffi::platform::global(), fd) {
        Ok(pgrp) => pgrp,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
