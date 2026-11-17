// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fseeko`/`ftello` and their 64-bit aliases.
//!
//! `off_t` is 64-bit on every RavenBSD target, so the 64-bit names are the same bodies.

use core::ffi::{c_int, c_void};

/// `fseeko`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fseeko(stream: *mut c_void, offset: i64, whence: c_int) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::fseek_impl(stream, offset, whence) }
}

/// `ftello`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ftello(stream: *mut c_void) -> i64 {
    // SAFETY: forwarded C contract.
    unsafe { super::ftell_impl(stream) }
}

/// `fseeko64`.
///
/// # Safety
/// As `fseeko`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fseeko64(stream: *mut c_void, offset: i64, whence: c_int) -> c_int {
    // SAFETY: as `fseeko`.
    unsafe { fseeko(stream, offset, whence) }
}

/// `ftello64`.
///
/// # Safety
/// As `ftello`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ftello64(stream: *mut c_void) -> i64 {
    // SAFETY: as `ftello`.
    unsafe { ftello(stream) }
}
