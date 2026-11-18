// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `mknodat(2)`.

use core::ffi::{c_char, c_int};

use raven_libc_abi::types::{dev_t, mode_t};

/// Create a special file relative to `dirfd`.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mknodat(
    dirfd: c_int,
    path: *const c_char,
    mode: mode_t,
    dev: dev_t,
) -> c_int {
    if path.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    let Ok(dev) = u32::try_from(dev) else {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EOVERFLOW as u16,
        )));
    };
    // SAFETY: the caller's NUL-terminated path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    crate::ffi::ret::platform_int(
        crate::ffi::platform::global()
            .mknodat(dirfd, path, mode, dev)
            .map(|()| 0),
    )
}
