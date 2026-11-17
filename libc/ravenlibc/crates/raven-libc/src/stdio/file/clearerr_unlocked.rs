// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The unlocked error/EOF/error-indicator/file-descriptor queries.

use core::ffi::{c_int, c_void};

/// `clearerr_unlocked`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearerr_unlocked(stream: *mut c_void) {
    // SAFETY: forwarded C contract.
    unsafe { super::clearerr_impl(stream) }
}

/// `feof_unlocked`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn feof_unlocked(stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::feof_impl(stream) }
}

/// `ferror_unlocked`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ferror_unlocked(stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::ferror_impl(stream) }
}

/// `fileno_unlocked`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fileno_unlocked(stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fileno_impl(stream) }
}
