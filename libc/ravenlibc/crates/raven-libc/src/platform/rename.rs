// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `rename(2)`.
use core::ffi::{c_char, c_int};

/// Rename `from` to `to`.
///
/// # Safety
/// Both pointers are NULL or NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rename(from: *const c_char, to: *const c_char) -> c_int {
    // SAFETY: the caller's C string contract.
    let from = unsafe { crate::ffi::cstr::slice(from) };
    // SAFETY: the caller's C string contract.
    let to = unsafe { crate::ffi::cstr::slice(to) };
    crate::ffi::ret::int(
        crate::ffi::platform::global()
            .rename(from, to)
            .map(|()| 0)
            .map_err(crate::platform::map_error),
    )
}
