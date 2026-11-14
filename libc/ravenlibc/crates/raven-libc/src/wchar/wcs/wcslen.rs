// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `wcslen`.
///
/// # Safety
/// `s` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcslen(s: *const u32) -> usize {
    // SAFETY: forwarded C contract.
    unsafe { super::wcslen_impl(s) }
}

/// `wcscpy`.
///
/// # Safety
/// `dst` is writable and `src` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcscpy(dst: *mut u32, src: *const u32) -> *mut u32 {
    // SAFETY: forwarded C contract.
    unsafe { super::wcscpy_impl(dst, src) }
}

/// `wcsncpy`.
///
/// # Safety
/// `dst` is writable for `n` units and `src` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsncpy(dst: *mut u32, src: *const u32, n: usize) -> *mut u32 {
    // SAFETY: forwarded C contract.
    unsafe { super::wcsncpy_impl(dst, src, n) }
}

/// `wcscat`.
///
/// # Safety
/// `dst` is NUL terminated and writable; `src` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcscat(dst: *mut u32, src: *const u32) -> *mut u32 {
    // SAFETY: forwarded C contract.
    unsafe { super::wcscat_impl(dst, src) }
}
