// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `putenv(3)`.
use core::ffi::{c_char, c_int};

/// Adopt the caller's `"NAME=value"` string.
///
/// # Safety
/// `text` is NUL terminated and stays valid for the life of the process.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putenv(text: *mut c_char) -> c_int {
    crate::ffi::ret::int(crate::stdlib::env_support::put(text).map(|()| 0))
}
