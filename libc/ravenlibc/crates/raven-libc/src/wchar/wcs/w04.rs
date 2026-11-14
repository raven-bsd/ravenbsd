// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::c_int;

/// `wcsstr`.
///
/// # Safety
/// Both arguments are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsstr(haystack: *const u32, needle: *const u32) -> *mut u32 {
    // SAFETY: forwarded C contract.
    unsafe { super::wcsstr_impl(haystack, needle) }
}

/// `wmemchr`.
///
/// # Safety
/// `s` is readable for `n` units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wmemchr(s: *const u32, unit: u32, n: usize) -> *mut u32 {
    // SAFETY: forwarded C contract.
    unsafe { super::wmemchr_impl(s, unit, n) }
}

/// `wmemcmp`.
///
/// # Safety
/// Both arguments are readable for `n` units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wmemcmp(a: *const u32, b: *const u32, n: usize) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::wmemcmp_impl(a, b, n) }
}

/// `wmemcpy`.
///
/// # Safety
/// `dst` and `src` are non-overlapping arrays readable/writable for `n` units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wmemcpy(dst: *mut u32, src: *const u32, n: usize) -> *mut u32 {
    // SAFETY: forwarded C contract.
    unsafe { super::wmemcpy_impl(dst, src, n) }
}
