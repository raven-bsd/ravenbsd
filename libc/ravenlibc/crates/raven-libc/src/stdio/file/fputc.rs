// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::{c_char, c_int, c_void};

/// `fputc`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fputc(c: c_int, stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fputc_impl(c, stream) }
}

/// `fgets`.
///
/// # Safety
/// `dst` is writable for `size` bytes and `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgets(dst: *mut c_char, size: c_int, stream: *mut c_void) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::fgets_impl(dst, size, stream) }
}

/// `fputs`.
///
/// # Safety
/// `text` is NUL terminated and `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fputs(text: *const c_char, stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fputs_impl(text, stream) }
}

/// `puts`.
///
/// # Safety
/// `text` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn puts(text: *const c_char) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::puts_impl(text) }
}
