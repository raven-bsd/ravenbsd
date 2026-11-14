// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::c_int;
use core::ffi::c_void;

/// `feof`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn feof(stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::feof_impl(stream) }
}

/// `ferror`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ferror(stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::ferror_impl(stream) }
}

/// `clearerr`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearerr(stream: *mut c_void) {
    // SAFETY: forwarded C contract.
    unsafe { super::clearerr_impl(stream) }
}

/// `fileno`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fileno(stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fileno_impl(stream) }
}
