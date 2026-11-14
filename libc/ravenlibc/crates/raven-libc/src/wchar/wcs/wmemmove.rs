// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::c_int;

/// `wmemmove`.
///
/// # Safety
/// `dst` and `src` are arrays readable/writable for `n` units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wmemmove(dst: *mut u32, src: *const u32, n: usize) -> *mut u32 {
    // SAFETY: forwarded C contract.
    unsafe { super::wmemmove_impl(dst, src, n) }
}

/// `wmemset`.
///
/// # Safety
/// `dst` is writable for `n` units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wmemset(dst: *mut u32, unit: u32, n: usize) -> *mut u32 {
    // SAFETY: forwarded C contract.
    unsafe { super::wmemset_impl(dst, unit, n) }
}

/// `wcscasecmp`.
///
/// # Safety
/// Both arguments are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcscasecmp(a: *const u32, b: *const u32) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::wcscasecmp_impl(a, b) }
}

/// `wcsncasecmp`.
///
/// # Safety
/// Both arguments are NUL terminated and readable for `n` units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsncasecmp(a: *const u32, b: *const u32, n: usize) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::wcsncasecmp_impl(a, b, n) }
}
