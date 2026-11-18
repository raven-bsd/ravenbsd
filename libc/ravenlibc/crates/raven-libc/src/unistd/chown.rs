// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `chown(2)`.

use core::ffi::{c_char, c_int};

use raven_libc_abi::types::{gid_t, uid_t};

/// Change the owner and group of `path`.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chown(path: *const c_char, owner: uid_t, group: gid_t) -> c_int {
    if path.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's NUL-terminated path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    crate::ffi::ret::platform_int(
        crate::ffi::platform::global()
            .chown(path, owner, group)
            .map(|()| 0),
    )
}
