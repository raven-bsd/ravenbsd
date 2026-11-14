// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::c_int;

/// `wcsncat`.
///
/// # Safety
/// `dst` is NUL terminated and writable; `src` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsncat(dst: *mut u32, src: *const u32, n: usize) -> *mut u32 {
    // SAFETY: forwarded C contract.
    unsafe { super::wcsncat_impl(dst, src, n) }
}

/// `wcscmp`.
///
/// # Safety
/// Both arguments are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcscmp(a: *const u32, b: *const u32) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::wcscmp_impl(a, b) }
}

/// `wcsncmp`.
///
/// # Safety
/// Both arguments are NUL terminated and readable for `n` units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsncmp(a: *const u32, b: *const u32, n: usize) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::wcsncmp_impl(a, b, n) }
}

/// `wcschr`.
///
/// # Safety
/// `s` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcschr(s: *const u32, unit: u32) -> *mut u32 {
    // SAFETY: forwarded C contract.
    unsafe { super::wcschr_impl(s, unit) }
}
