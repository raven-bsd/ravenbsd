// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `creat(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_abi::fcntl::{O_CREAT, O_TRUNC, O_WRONLY};

/// Create or truncate `path` with `mode`.
///
/// # Safety
/// `path` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn creat(path: *const c_char, mode: c_int) -> c_int {
    if path.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    let flags = (O_CREAT | O_WRONLY | O_TRUNC) as u32;
    let result = crate::ffi::platform::global().open(path, flags, mode as u32);
    crate::ffi::ret::platform_int(result)
}
