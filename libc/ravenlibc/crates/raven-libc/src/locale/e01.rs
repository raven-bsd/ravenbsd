// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::{c_char, c_int, c_void};

/// `setlocale`.
///
/// # Safety
/// `name` is NULL or NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setlocale(cat: c_int, name: *const c_char) -> *mut c_char {
    // SAFETY: forwarded C string contract.
    unsafe { super::setlocale_impl(cat, name) }
}

/// `newlocale`.
///
/// # Safety
/// `name` is NUL terminated; `base` is a locale or `LC_GLOBAL_LOCALE`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn newlocale(
    mask: c_int,
    name: *const c_char,
    base: *mut c_void,
) -> *mut c_void {
    // SAFETY: forwarded C contracts.
    unsafe { super::newlocale_impl(mask, name, base) }
}

/// `uselocale`.
///
/// # Safety
/// `locale` is a locale or `LC_GLOBAL_LOCALE`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn uselocale(locale: *mut c_void) -> *mut c_void {
    // SAFETY: forwarded C contract.
    unsafe { super::uselocale_impl(locale) }
}

/// `duplocale`.
///
/// # Safety
/// `locale` is a locale or `LC_GLOBAL_LOCALE`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn duplocale(locale: *mut c_void) -> *mut c_void {
    // SAFETY: forwarded C contract.
    unsafe { super::duplocale_impl(locale) }
}
