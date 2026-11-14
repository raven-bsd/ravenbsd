// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::{c_char, c_int, c_void};

/// `fopen`.
///
/// # Safety
/// `path` and `mode` are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fopen(path: *const c_char, mode: *const c_char) -> *mut c_void {
    // SAFETY: forwarded C contract.
    unsafe { super::fopen_impl(path, mode).cast() }
}

/// `fdopen`.
///
/// # Safety
/// `mode` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fdopen(fd: c_int, mode: *const c_char) -> *mut c_void {
    // SAFETY: forwarded C contract.
    unsafe { super::fdopen_impl(fd, mode).cast() }
}

/// `freopen`.
///
/// # Safety
/// `path` and `mode` follow the C string contract; `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freopen(
    path: *const c_char,
    mode: *const c_char,
    stream: *mut c_void,
) -> *mut c_void {
    // SAFETY: forwarded C contract.
    unsafe { super::freopen_impl(path, mode, stream).cast() }
}

/// `fclose`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fclose(stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fclose_impl(stream) }
}
