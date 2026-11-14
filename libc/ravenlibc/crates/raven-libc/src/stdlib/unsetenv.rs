// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `unsetenv(3)`.
use core::ffi::{c_char, c_int};

/// Remove `name` from the environment.
///
/// # Safety
/// `name` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unsetenv(name: *const c_char) -> c_int {
    // SAFETY: the caller's name contract.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    crate::ffi::ret::int(crate::stdlib::env_support::unset(name).map(|()| 0))
}
