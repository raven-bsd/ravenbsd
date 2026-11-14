// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `a64l(3)`.
use core::ffi::{c_char, c_long};

/// Decode a base-64 string.
///
/// # Safety
/// `text` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn a64l(text: *const c_char) -> c_long {
    // SAFETY: the caller's string contract.
    let text = unsafe { crate::ffi::cstr::slice(text) };
    raven_libc_stdlib::misc::a64l(text).unwrap_or(-1)
}
