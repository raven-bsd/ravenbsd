// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P1c exports, grouped so each codegen unit stays within the K4 C-symbol budget.

use super::*;

/// Create a directory relative to `dirfd`.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn mkdirat(dirfd: c_int, path: *const c_char, mode: u32) -> c_int {
    if path.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    unit(raven_libc_platform::compat::mkdirat(
        global(),
        dirfd,
        path,
        mode,
    ))
}
/// Read file-system statistics for an open descriptor.
///
/// # Safety
/// `buf` points to a writable `struct statvfs`.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn fstatvfs(fd: c_int, buf: *mut Statvfs) -> c_int {
    if buf.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    match global().fstatvfs(fd) {
        Ok(value) => {
            // SAFETY: the caller supplied a writable Statvfs.
            unsafe { *buf = value };
            0
        }
        Err(error) => ret::int(Err(map_error(error))),
    }
}
/// Change the working directory by descriptor.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn fchdir(fd: c_int) -> c_int {
    unit(global().fchdir(fd))
}
/// Flush file data.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn fdatasync(fd: c_int) -> c_int {
    unit(raven_libc_platform::compat::fdatasync(global(), fd))
}
