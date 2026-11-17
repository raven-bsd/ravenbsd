// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `statvfs(2)`.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EFAULT, Errno};
use raven_libc_abi::structs::Statvfs;

/// `statvfs`: fill `buf` with the filesystem statistics of `path`.
///
/// # Safety
/// `path` is NUL-terminated and `buf` points to a writable `struct statvfs`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn statvfs(path: *const c_char, buf: *mut Statvfs) -> c_int {
    if path.is_null() || buf.is_null() {
        return crate::ffi::ret::int(Err(Errno(EFAULT as u16)));
    }
    // SAFETY: the caller's path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    match crate::ffi::platform::global().statvfs(path) {
        Ok(value) => {
            // SAFETY: `buf` is the caller's writable struct statvfs.
            unsafe { core::ptr::write(buf, value) };
            0
        }
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
