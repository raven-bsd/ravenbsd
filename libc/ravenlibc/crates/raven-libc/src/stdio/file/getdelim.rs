// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `getdelim` C-ABI export.

use core::ffi::{c_char, c_int, c_void};

/// `getdelim`.
///
/// # Safety
/// `lineptr` and `capacity` are writable, `*lineptr` is NULL or a live malloc block, and
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getdelim(
    lineptr: *mut *mut c_char,
    capacity: *mut usize,
    delim: c_int,
    stream: *mut c_void,
) -> isize {
    // SAFETY: forwarded C contract.
    unsafe { super::getdelim_impl(lineptr, capacity, delim, stream) }
}
