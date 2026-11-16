// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `remove` C-ABI export.

use core::ffi::{c_char, c_int};

/// `remove`.
///
/// # Safety
/// `path` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remove(path: *const c_char) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { super::remove_impl(path) }
}
