// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `unlink(2)`.
use core::ffi::{c_char, c_int};

/// Remove the directory entry `path`.
///
/// # Safety
/// `path` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unlink(path: *const c_char) -> c_int {
    if path.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    crate::ffi::ret::platform_int(crate::ffi::platform::global().unlink(path).map(|()| 0))
}
