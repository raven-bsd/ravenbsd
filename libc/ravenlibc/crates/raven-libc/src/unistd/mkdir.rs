// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mkdir(2)`.

use core::ffi::{c_char, c_int};

use raven_libc_abi::types::mode_t;

/// Create the directory `path` with permission bits `mode`.
///
/// The platform resolves the path and applies the process umask before the
/// `OP_MKDIR` request. A pointer outside the user window is rejected with
/// `EFAULT`; a path without a NUL within `PATH_MAX` with `ENAMETOOLONG`.
///
/// # Safety
/// `path` is NUL terminated or outside the user window.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mkdir(path: *const c_char, mode: mode_t) -> c_int {
    // SAFETY: the caller's NUL-terminated path contract; an out-of-window pointer is `EFAULT`.
    let path = match unsafe { crate::ffi::cstr::checked_path(path) } {
        Ok(path) => path,
        Err(error) => return crate::ffi::ret::int(Err(error)),
    };
    crate::ffi::ret::platform_int(crate::ffi::platform::global().mkdir(path, mode).map(|()| 0))
}
