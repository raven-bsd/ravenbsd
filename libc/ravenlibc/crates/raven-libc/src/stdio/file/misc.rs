// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `remove` and `rewind` C-ABI exports.

use core::ffi::{c_char, c_int, c_void};

/// `remove`.
///
/// # Safety
/// `path` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remove(path: *const c_char) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::remove_impl(path) }
}

/// `rewind`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rewind(stream: *mut c_void) {
    // SAFETY: forwarded C contract.
    unsafe { super::rewind_impl(stream) }
}
