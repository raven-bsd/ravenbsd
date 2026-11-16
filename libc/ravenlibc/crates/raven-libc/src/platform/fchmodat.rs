// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fchmodat(2)`.
use core::ffi::{c_char, c_int, c_uint};

/// Change the mode of a path relative to `dirfd`; no `*at` primitive exists yet.
///
/// # Safety
/// `path` is NULL or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fchmodat(
    _dirfd: c_int,
    _path: *const c_char,
    _mode: c_uint,
    _flags: c_int,
) -> c_int {
    crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
        raven_libc_abi::errno::ENOSYS as u16,
    )))
}
