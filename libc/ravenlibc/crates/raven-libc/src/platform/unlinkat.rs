// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `unlinkat(2)`.
use core::ffi::{c_char, c_int};

/// Remove a path relative to `dirfd`.
///
/// # Safety
/// `path` is NULL or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unlinkat(dirfd: c_int, path: *const c_char, flags: c_int) -> c_int {
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    crate::ffi::ret::int(
        crate::ffi::platform::global()
            .unlinkat(dirfd, path, flags)
            .map(|()| 0)
            .map_err(crate::platform::map_error),
    )
}
