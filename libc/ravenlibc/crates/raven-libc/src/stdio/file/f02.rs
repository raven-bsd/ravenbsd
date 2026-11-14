// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::{c_int, c_void};

/// `fflush`.
///
/// # Safety
/// `stream` is NULL or a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fflush(stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fflush_impl(stream) }
}

/// `fread`.
///
/// # Safety
/// `ptr` is writable for `size * nmemb` bytes and `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fread(
    ptr: *mut c_void,
    size: usize,
    nmemb: usize,
    stream: *mut c_void,
) -> usize {
    // SAFETY: forwarded C contract.
    unsafe { super::fread_impl(ptr, size, nmemb, stream) }
}

/// `fwrite`.
///
/// # Safety
/// `ptr` is readable for `size * nmemb` bytes and `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fwrite(
    ptr: *const c_void,
    size: usize,
    nmemb: usize,
    stream: *mut c_void,
) -> usize {
    // SAFETY: forwarded C contract.
    unsafe { super::fwrite_impl(ptr, size, nmemb, stream) }
}

/// `fseek`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fseek(stream: *mut c_void, offset: i64, whence: c_int) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fseek_impl(stream, offset, whence) }
}
