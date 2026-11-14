// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `stat(2)`.
use core::ffi::{c_char, c_int};

use raven_libc_abi::structs::Stat;

/// Fill `buf` with the metadata of `path`.
///
/// # Safety
/// `path` is NUL terminated; `buf` points to a writable `struct stat`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stat(path: *const c_char, buf: *mut Stat) -> c_int {
    if path.is_null() || buf.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    match crate::ffi::platform::global().stat(path) {
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
