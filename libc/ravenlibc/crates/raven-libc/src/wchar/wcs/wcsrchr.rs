// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `wcsrchr`.
///
/// # Safety
/// `s` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsrchr(s: *const u32, unit: u32) -> *mut u32 {
    // SAFETY: forwarded C contract.
    unsafe { super::wcsrchr_impl(s, unit) }
}

/// `wcscspn`.
///
/// # Safety
/// Both arguments are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcscspn(s: *const u32, reject: *const u32) -> usize {
    // SAFETY: forwarded C contract.
    unsafe { super::wcscspn_impl(s, reject) }
}

/// `wcsspn`.
///
/// # Safety
/// Both arguments are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsspn(s: *const u32, accept: *const u32) -> usize {
    // SAFETY: forwarded C contract.
    unsafe { super::wcsspn_impl(s, accept) }
}

/// `wcspbrk`.
///
/// # Safety
/// Both arguments are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcspbrk(s: *const u32, accept: *const u32) -> *mut u32 {
    // SAFETY: forwarded C contract.
    unsafe { super::wcspbrk_impl(s, accept) }
}
