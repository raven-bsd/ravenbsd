// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::c_int;

/// `wcslcpy`.
///
/// # Safety
/// `dst` is writable for `size` units and `src` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcslcpy(dst: *mut u32, src: *const u32, size: usize) -> usize {
    // SAFETY: forwarded C contract.
    unsafe { super::wcslcpy_impl(dst, src, size) }
}

/// `wcslcat`.
///
/// # Safety
/// `dst` is a NUL-terminated buffer of `size` units; `src` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcslcat(dst: *mut u32, src: *const u32, size: usize) -> usize {
    // SAFETY: forwarded C contract.
    unsafe { super::wcslcat_impl(dst, src, size) }
}

/// `wcswidth`.
///
/// # Safety
/// `s` is NUL terminated and readable for `n` units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcswidth(s: *const u32, n: usize) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::wcswidth_impl(s, n) }
}
