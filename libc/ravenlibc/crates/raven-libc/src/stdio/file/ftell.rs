// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::{c_char, c_int, c_void};

/// `ftell`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ftell(stream: *mut c_void) -> i64 {
    // SAFETY: forwarded C contract.
    unsafe { super::ftell_impl(stream) }
}

/// `setvbuf`.
///
/// # Safety
/// `stream` is a FILE and `buf` is NULL or writable for `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setvbuf(
    stream: *mut c_void,
    buf: *mut c_char,
    mode: c_int,
    size: usize,
) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::setvbuf_impl(stream, buf, mode, size) }
}

/// `ungetc`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ungetc(c: c_int, stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::ungetc_impl(c, stream) }
}

/// `fgetc`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgetc(stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fgetc_impl(stream) }
}
