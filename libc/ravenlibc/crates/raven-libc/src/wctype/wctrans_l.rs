// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `wctrans_l` and `wctype_l`.

use core::ffi::c_char;

/// Locale-aware mapping descriptor lookup.
///
/// # Safety
/// `name` is NULL or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wctrans_l(name: *const c_char, _locale: *mut core::ffi::c_void) -> u64 {
    if name.is_null() {
        return 0;
    }
    // SAFETY: the caller's string contract.
    raven_libc_ctype::wctrans(unsafe { crate::ffi::cstr::slice(name) })
}

/// Locale-aware classification descriptor lookup.
///
/// # Safety
/// `name` is NULL or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wctype_l(name: *const c_char, _locale: *mut core::ffi::c_void) -> u64 {
    if name.is_null() {
        return 0;
    }
    // SAFETY: the caller's string contract.
    raven_libc_ctype::wctype(unsafe { crate::ffi::cstr::slice(name) })
}
