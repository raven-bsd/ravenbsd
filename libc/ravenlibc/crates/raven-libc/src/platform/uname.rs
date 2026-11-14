// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `uname(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Utsname;

/// Fill `buf` with the system identification.
///
/// # Safety
/// `buf` points to a writable `struct utsname`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn uname(buf: *mut Utsname) -> c_int {
    if buf.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    match crate::ffi::platform::global().uname() {
        // SAFETY: `buf` is the caller's writable utsname.
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
