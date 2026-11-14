// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getenv_r(3)`.
use core::ffi::{c_char, c_int};

/// Copy the value of `name` into `buf`.
///
/// # Safety
/// `name` is NUL terminated; `buf` points to `size` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getenv_r(name: *const c_char, buf: *mut c_char, size: usize) -> c_int {
    // SAFETY: the caller's name and buffer contracts.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    // SAFETY: as above.
    let out = unsafe { crate::ffi::cstr::slice_mut(buf, size) };
    let env = unsafe { crate::stdlib::env_support::table() };
    crate::ffi::ret::int(env.getenv_r(name, out).map(|_| 0))
}
