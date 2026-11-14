// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setenv(3)`.
use core::ffi::{c_char, c_int};

/// Set `name` to `value`.
///
/// # Safety
/// `name` and `value` are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setenv(
    name: *const c_char,
    value: *const c_char,
    overwrite: c_int,
) -> c_int {
    // SAFETY: the caller's string contracts.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    // SAFETY: as above.
    let value = unsafe { crate::ffi::cstr::slice(value) };
    crate::ffi::ret::int(crate::stdlib::env_support::set(name, value, overwrite != 0).map(|()| 0))
}
