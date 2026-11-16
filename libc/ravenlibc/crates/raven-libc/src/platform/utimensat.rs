// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `utimensat(2)`.
use core::ffi::{c_char, c_int, c_void};

/// Set the times of a path relative to `dirfd`; the `UTIMES` call is not wired yet (PG-31 stub).
///
/// # Safety
/// The pointers are not dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn utimensat(
    _dirfd: c_int,
    _path: *const c_char,
    _times: *const c_void,
    _flags: c_int,
) -> c_int {
    crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
        raven_libc_abi::errno::ENOSYS as u16,
    )))
}
