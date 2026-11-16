// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `fgetln` and `getline` C-ABI exports.

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
