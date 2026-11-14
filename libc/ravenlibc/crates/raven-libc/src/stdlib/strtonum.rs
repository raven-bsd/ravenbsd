// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strtonum(3)`.
use core::ffi::{c_char, c_longlong};

use raven_libc_stdlib::conv::StrtonumError;

static INVALID: &[u8] = b"invalid\0";
static TOO_SMALL: &[u8] = b"too small\0";
static TOO_LARGE: &[u8] = b"too large\0";

/// Parse `text` into the inclusive range `[low, high]`.
///
/// # Safety
/// `text` is NUL terminated; `errstr` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtonum(
    text: *const c_char,
    low: c_longlong,
    high: c_longlong,
    errstr: *mut *const c_char,
) -> c_longlong {
    // SAFETY: the caller's string contract.
    let text = unsafe { crate::ffi::cstr::slice(text) };
    let result = raven_libc_stdlib::strtonum(text, low, high);
    if !errstr.is_null() {
        // SAFETY: the caller's writable error-string slot.
        unsafe { *errstr = error_text(result.as_ref().err()) };
    }
    result.unwrap_or(0)
}

/// The static text for a `strtonum` failure.
fn error_text(error: Option<&StrtonumError>) -> *const c_char {
    match error {
        Some(StrtonumError::TooSmall) => TOO_SMALL.as_ptr().cast::<c_char>(),
        Some(StrtonumError::TooLarge) => TOO_LARGE.as_ptr().cast::<c_char>(),
        Some(StrtonumError::Invalid) => INVALID.as_ptr().cast::<c_char>(),
        None => core::ptr::null(),
    }
}
