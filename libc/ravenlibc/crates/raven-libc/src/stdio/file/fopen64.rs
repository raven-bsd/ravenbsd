// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fopen64` and `freopen64`: the 64-bit aliases.

use core::ffi::{c_char, c_void};

/// `fopen64`: the 64-bit alias of `fopen`.
///
/// # Safety
/// As `fopen`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fopen64(path: *const c_char, mode: *const c_char) -> *mut c_void {
    // SAFETY: as `fopen`.
    unsafe { super::fopen_impl(path, mode).cast() }
}

/// `freopen64`: the 64-bit alias of `freopen`.
///
/// # Safety
/// As `freopen`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freopen64(
    path: *const c_char,
    mode: *const c_char,
    stream: *mut c_void,
) -> *mut c_void {
    // SAFETY: as `freopen`.
    unsafe { super::freopen_impl(path, mode, stream).cast() }
}
