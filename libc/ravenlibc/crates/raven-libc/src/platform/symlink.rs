// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `symlink(2)`.
use core::ffi::{c_char, c_int};

/// Create `link` pointing at `target`.
///
/// # Safety
/// Both pointers are NULL or NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn symlink(target: *const c_char, link: *const c_char) -> c_int {
    // SAFETY: the caller's C string contract.
    let target = unsafe { crate::ffi::cstr::slice(target) };
    // SAFETY: the caller's C string contract.
    let link = unsafe { crate::ffi::cstr::slice(link) };
    crate::ffi::ret::int(
        crate::ffi::platform::global()
            .symlink(target, link)
            .map(|()| 0)
            .map_err(crate::platform::map_error),
    )
}
