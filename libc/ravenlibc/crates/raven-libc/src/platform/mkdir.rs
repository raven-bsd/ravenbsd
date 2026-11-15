// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mkdir(2)`.
use core::ffi::{c_char, c_int, c_uint};

/// Create `path` with `mode`.
///
/// # Safety
/// `path` is NULL or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mkdir(path: *const c_char, mode: c_uint) -> c_int {
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    crate::ffi::ret::int(
        crate::ffi::platform::global()
            .mkdir(path, mode)
            .map(|()| 0)
            .map_err(crate::platform::map_error),
    )
}
