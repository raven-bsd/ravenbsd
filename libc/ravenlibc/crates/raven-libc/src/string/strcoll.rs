// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strcoll(3)`, `strxfrm(3)` and their `_l` forms.
//!
//! The only implemented collation is the C/UTF-8 byte order, so `strcoll` is `strcmp` and
//! `strxfrm` is a bounded `strlcpy`. The `_l` forms accept a locale object and use the same
//! byte-order collation; a locale-aware collation is a later locale batch.

use core::ffi::{c_char, c_int, c_void};

/// `strcoll`: byte-order collation.
///
/// # Safety
/// Both pointers are NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcoll(a: *const c_char, b: *const c_char) -> c_int {
    // SAFETY: the caller's `strcmp` contract.
    unsafe { raven_libc_str::raw::strcmp(a.cast(), b.cast()) }
}

/// `strxfrm`: copy at most `n` bytes and return `strlen(src)`.
///
/// # Safety
/// `dst` is writable for `n` bytes (`n == 0` allows NULL); `src` is NUL-terminated; no overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strxfrm(dst: *mut c_char, src: *const c_char, n: usize) -> usize {
    // SAFETY: the caller's `strlcpy` contract.
    unsafe { raven_libc_str::raw::strlcpy(dst.cast(), src.cast(), n) }
}

/// `strcoll_l`: the C/UTF-8 byte order; the locale object is accepted and ignored.
///
/// # Safety
/// As `strcoll`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcoll_l(
    a: *const c_char,
    b: *const c_char,
    _locale: *mut c_void,
) -> c_int {
    // SAFETY: as `strcoll`.
    unsafe { strcoll(a, b) }
}

/// `strxfrm_l`: the C/UTF-8 byte order; the locale object is accepted and ignored.
///
/// # Safety
/// As `strxfrm`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strxfrm_l(
    dst: *mut c_char,
    src: *const c_char,
    n: usize,
    _locale: *mut c_void,
) -> usize {
    // SAFETY: as `strxfrm`.
    unsafe { strxfrm(dst, src, n) }
}
