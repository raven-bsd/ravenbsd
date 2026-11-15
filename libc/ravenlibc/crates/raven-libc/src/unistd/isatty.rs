// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `isatty(2)`.

use core::ffi::c_int;

/// Return 1 when `fd` is a terminal, 0 otherwise.
///
/// The platform maps a non-tty server's `ENOSYS` to `ENOTTY`, so `isatty`
/// never reports a regular file or a bad descriptor as a terminal.
///
/// # Safety
/// `fd` is a descriptor owned by the caller; no pointer is dereferenced.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn isatty(fd: c_int) -> c_int {
    c_int::from(raven_libc_platform::tty::isatty(
        crate::ffi::platform::global(),
        fd,
    ))
}
