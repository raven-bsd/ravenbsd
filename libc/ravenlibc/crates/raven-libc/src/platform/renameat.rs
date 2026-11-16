// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `renameat(2)`.
use core::ffi::{c_char, c_int};

const AT_FDCWD: c_int = -100;

/// Rename a path relative to two directories; cwd-relative calls use `rename`.
///
/// # Safety
/// Both path pointers are NULL or NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn renameat(
    olddirfd: c_int,
    oldpath: *const c_char,
    newdirfd: c_int,
    newpath: *const c_char,
) -> c_int {
    if olddirfd != AT_FDCWD || newdirfd != AT_FDCWD {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::ENOSYS as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let oldpath = unsafe { crate::ffi::cstr::slice(oldpath) };
    // SAFETY: the caller's C string contract.
    let newpath = unsafe { crate::ffi::cstr::slice(newpath) };
    crate::ffi::ret::int(
        crate::ffi::platform::global()
            .rename(oldpath, newpath)
            .map(|()| 0)
            .map_err(crate::platform::map_error),
    )
}
