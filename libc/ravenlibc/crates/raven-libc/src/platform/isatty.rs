// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `isatty(3)`.
use core::ffi::c_int;

/// Whether `fd` is a terminal; a non-tty sets `ENOTTY` like the C library.
#[unsafe(no_mangle)]
pub extern "C" fn isatty(fd: c_int) -> c_int {
    if raven_libc_platform::tty::isatty(crate::ffi::platform::global(), fd) {
        1
    } else {
        crate::ffi::errno::set(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::ENOTTY as u16,
        ));
        0
    }
}
