// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fstatat(2)`.

use core::ffi::{c_char, c_int};

use raven_libc_abi::structs::Stat;

/// Fill `buf` with the metadata of `path` relative to `dirfd`.
///
/// `AT_SYMLINK_NOFOLLOW` and `AT_EMPTY_PATH` are handed to the platform's
/// `stat_at`; the other flag bits are part of the same Linux/generated set and
/// are likewise passed through. The platform owns the supported subset.
///
/// # Safety
/// `path` is a NUL-terminated string and `buf` points to a writable `struct stat`.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fstatat(
    dirfd: c_int,
    path: *const c_char,
    buf: *mut Stat,
    flags: c_int,
) -> c_int {
    if path.is_null() || buf.is_null() {
        crate::ffi::errno::set(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        ));
        return -1;
    }
    // SAFETY: the caller's NUL-terminated path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    match crate::ffi::platform::global().stat_at(dirfd, path, flags) {
        // SAFETY: `buf` is the caller's writable struct stat.
        Ok(value) => {
            unsafe { core::ptr::write(buf, value) };
            0
        }
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
