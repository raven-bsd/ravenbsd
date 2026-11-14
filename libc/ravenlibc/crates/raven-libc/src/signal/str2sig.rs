// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `str2sig(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_signal::sig2str::str2sig as str2sig_core;

/// Parse a signal name or decimal number.
///
/// # Safety
/// `text` is NUL terminated; `sig` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn str2sig(text: *const c_char, sig: *mut c_int) -> c_int {
    if text.is_null() || sig.is_null() {
        return -1;
    }
    // SAFETY: the caller supplies a NUL-terminated string.
    let bytes = unsafe { crate::ffi::cstr::slice(text) };
    match str2sig_core(bytes) {
        Ok(value) => {
            // SAFETY: the caller supplies a writable int.
            unsafe { *sig = value };
            0
        }
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
