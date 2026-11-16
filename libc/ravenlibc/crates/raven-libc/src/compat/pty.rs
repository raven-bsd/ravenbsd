// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The pty C-ABI exports.
//!
//! RavenBSD has no pty scheme yet (PG-54), so the four entry points are explicit `ENOSYS`
//! placeholders: they link and report the missing primitive instead of pretending that a
//! `/dev/ptmx`-style node exists. The `openpty`/`forkpty`/`login_tty` logic above them is
//! already in `raven-libc-compat` and starts working when PG-54 lands.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::Errno;

/// `ENOSYS` in the C library errno space.
const ENOSYS: u16 = 38;

fn unsupported() -> c_int {
    crate::ffi::errno::set(Errno(ENOSYS));
    -1
}

/// `posix_openpt` (PG-54).
///
/// # Safety
/// Scalar arguments only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_openpt(_flags: c_int) -> c_int {
    unsupported()
}

/// `grantpt` (PG-54).
///
/// # Safety
/// Scalar arguments only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn grantpt(_fd: c_int) -> c_int {
    unsupported()
}

/// `unlockpt` (PG-54).
///
/// # Safety
/// Scalar arguments only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unlockpt(_fd: c_int) -> c_int {
    unsupported()
}

/// `ptsname_r` (PG-54).
///
/// # Safety
/// `buffer` is NULL or writable for `length` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ptsname_r(_fd: c_int, _buffer: *mut c_char, _length: usize) -> c_int {
    unsupported()
}
