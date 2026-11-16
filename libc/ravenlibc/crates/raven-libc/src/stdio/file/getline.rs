// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `getline` C-ABI export.

use core::ffi::{c_char, c_void};

/// `getline`.
///
/// # Safety
/// `lineptr` and `capacity` are writable, `*lineptr` is NULL or a live malloc block, and
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getline(
    lineptr: *mut *mut c_char,
    capacity: *mut usize,
    stream: *mut c_void,
) -> isize {
    // SAFETY: forwarded C contract.
    unsafe { super::getline_impl(lineptr, capacity, stream) }
}
