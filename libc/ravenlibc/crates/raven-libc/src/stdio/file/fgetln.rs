// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `fgetln` C-ABI export.

use core::ffi::{c_char, c_void};

/// `fgetln`.
///
/// # Safety
/// `stream` is a FILE and `length` is NULL or writable for one `size_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgetln(stream: *mut c_void, length: *mut usize) -> *mut c_char {
    // SAFETY: forwarded C contract.
    unsafe { super::fgetln_impl(stream, length) }
}
