// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `putc` and its `fputc` aliases.

use core::ffi::{c_int, c_void};

/// `putc`: write one byte, like `fputc`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putc(c: c_int, stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fputc_impl(c, stream) }
}

/// `putc_unlocked`.
///
/// # Safety
/// As `putc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putc_unlocked(c: c_int, stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fputc_impl(c, stream) }
}

/// `fputc_unlocked`.
///
/// # Safety
/// As `putc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fputc_unlocked(c: c_int, stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fputc_impl(c, stream) }
}
