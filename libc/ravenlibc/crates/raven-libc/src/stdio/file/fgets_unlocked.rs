// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fgets_unlocked` and `fputs_unlocked`.

use core::ffi::{c_char, c_int, c_void};

/// `fgets_unlocked`.
///
/// # Safety
/// As `fgets`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgets_unlocked(
    dst: *mut c_char,
    size: c_int,
    stream: *mut c_void,
) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::fgets_impl(dst, size, stream) }
}

/// `fputs_unlocked`.
///
/// # Safety
/// As `fputs`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fputs_unlocked(text: *const c_char, stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fputs_impl(text, stream) }
}
