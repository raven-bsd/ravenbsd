// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getc` and its `fgetc` aliases.
//!
//! RavenBSD's stdio state has one lock domain; the unlocked entry points are the same body as
//! their locked forms.

use core::ffi::{c_int, c_void};

/// `getc`: read one byte, like `fgetc`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getc(stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fgetc_impl(stream) }
}

/// `getc_unlocked`.
///
/// # Safety
/// As `getc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getc_unlocked(stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fgetc_impl(stream) }
}

/// `fgetc_unlocked`.
///
/// # Safety
/// As `getc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgetc_unlocked(stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fgetc_impl(stream) }
}
