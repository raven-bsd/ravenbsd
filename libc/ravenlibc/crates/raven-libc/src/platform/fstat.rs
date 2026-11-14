// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fstat(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Stat;

/// Fill `buf` with the metadata of `fd`.
///
/// # Safety
/// `buf` points to a writable `struct stat`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fstat(fd: c_int, buf: *mut Stat) -> c_int {
    if buf.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    match crate::ffi::platform::global().fstat(fd) {
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
