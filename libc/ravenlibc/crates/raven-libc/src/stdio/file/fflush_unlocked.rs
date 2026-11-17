// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fflush_unlocked`, `fread_unlocked` and `fwrite_unlocked`.

use core::ffi::c_void;

/// `fflush_unlocked`.
///
/// # Safety
/// `stream` is NULL or a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fflush_unlocked(stream: *mut c_void) -> core::ffi::c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fflush_impl(stream) }
}

/// `fread_unlocked`.
///
/// # Safety
/// As `fread`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fread_unlocked(
    ptr: *mut c_void,
    size: usize,
    nmemb: usize,
    stream: *mut c_void,
) -> usize {
    // SAFETY: forwarded C contract.
    unsafe { super::fread_impl(ptr, size, nmemb, stream) }
}

/// `fwrite_unlocked`.
///
/// # Safety
/// As `fwrite`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fwrite_unlocked(
    ptr: *const c_void,
    size: usize,
    nmemb: usize,
    stream: *mut c_void,
) -> usize {
    // SAFETY: forwarded C contract.
    unsafe { super::fwrite_impl(ptr, size, nmemb, stream) }
}
