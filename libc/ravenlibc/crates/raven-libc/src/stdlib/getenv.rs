// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getenv(3)`.
use core::ffi::c_char;

/// The value of `name`, or NULL.
///
/// # Safety
/// `name` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getenv(name: *const c_char) -> *mut c_char {
    // SAFETY: the caller's name contract.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    match crate::stdlib::env_support::get(name) {
        Some(value) => value.as_ptr().cast::<c_char>().cast_mut(),
        None => core::ptr::null_mut(),
    }
}
