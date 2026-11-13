// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strcasecmp(3)`, `strncasecmp(3)` and their `_l` forms.
use core::ffi::{c_char, c_int};

/// Case-insensitive byte comparison.
///
/// # Safety
/// Both pointers are readable NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcasecmp(a: *const c_char, b: *const c_char) -> c_int {
    // SAFETY: the caller's contract is the strcasecmp(3) contract.
    unsafe { raven_libc_str::raw::strcasecmp(a.cast(), b.cast()) }
}

/// Case-insensitive bounded comparison.
///
/// # Safety
/// Both pointers grant `n` readable bytes or contain a NUL before them.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncasecmp(a: *const c_char, b: *const c_char, n: usize) -> c_int {
    // SAFETY: the caller's contract is the strncasecmp(3) contract.
    unsafe { raven_libc_str::raw::strncasecmp(a.cast(), b.cast(), n) }
}

/// The `_l` form; the locale argument is ignored until C48 lands.
///
/// # Safety
/// Both pointers are readable NUL-terminated strings; `locale` is opaque.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcasecmp_l(
    a: *const c_char,
    b: *const c_char,
    _locale: *mut core::ffi::c_void,
) -> c_int {
    // SAFETY: the caller's contract is strcasecmp(3); the locale is unused.
    unsafe { raven_libc_str::raw::strcasecmp(a.cast(), b.cast()) }
}

/// The bounded `_l` form; the locale argument is ignored until C48 lands.
///
/// # Safety
/// Both pointers grant `n` readable bytes or contain a NUL before them.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncasecmp_l(
    a: *const c_char,
    b: *const c_char,
    n: usize,
    _locale: *mut core::ffi::c_void,
) -> c_int {
    // SAFETY: the caller's contract is strncasecmp(3); the locale is unused.
    unsafe { raven_libc_str::raw::strncasecmp(a.cast(), b.cast(), n) }
}
