// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::{c_char, c_void};

/// `perror`.
///
/// # Safety
/// `message` is NULL or NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn perror(message: *const c_char) {
    // SAFETY: forwarded C contract.
    unsafe { super::perror_impl(message) }
}

/// `fmemopen`.
///
/// # Safety
/// `buffer` is NULL or writable for `size` bytes; `mode` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmemopen(
    buffer: *mut c_void,
    size: usize,
    mode: *const c_char,
) -> *mut c_void {
    // SAFETY: forwarded C contract.
    unsafe { super::fmemopen_impl(buffer, size, mode).cast() }
}

/// `open_memstream`.
///
/// # Safety
/// `buffer` and `size` are non-NULL writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn open_memstream(buffer: *mut *mut c_char, size: *mut usize) -> *mut c_void {
    // SAFETY: forwarded C contract.
    unsafe { super::open_memstream_impl(buffer, size).cast() }
}
