// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `linkat(2)`.
use core::ffi::{c_char, c_int};

/// Create a hard link relative to `dirfd`; not exposed by the filesystem call set yet.
///
/// # Safety
/// Both path pointers are NULL or NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn linkat(
    _olddirfd: c_int,
    _oldpath: *const c_char,
    _newdirfd: c_int,
    _newpath: *const c_char,
    _flags: c_int,
) -> c_int {
    crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
        raven_libc_abi::errno::ENOSYS as u16,
    )))
}
