// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `rename(2)`.

use core::ffi::{c_char, c_int};

/// Rename `from` to `to`, replacing `to` when it exists.
///
/// Both paths are resolved against the current directory; the platform rejects
/// a trailing slash on `from` when the object is not a directory.
///
/// # Safety
/// Both pointers are NUL terminated.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rename(from: *const c_char, to: *const c_char) -> c_int {
    if from.is_null() || to.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's NUL-terminated path contract.
    let from = unsafe { crate::ffi::cstr::slice(from) };
    // SAFETY: the caller's NUL-terminated path contract.
    let to = unsafe { crate::ffi::cstr::slice(to) };
    crate::ffi::ret::platform_int(crate::ffi::platform::global().rename(from, to).map(|()| 0))
}
