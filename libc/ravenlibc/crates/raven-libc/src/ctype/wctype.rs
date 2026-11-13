// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wctype(3)` and `iswctype(3)`.
use core::ffi::{c_char, c_int};

/// Resolve a class name to its `wctype_t`.
///
/// # Safety
/// `name` is a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wctype(name: *const c_char) -> u64 {
    // SAFETY: the caller's string contract.
    raven_libc_ctype::wctype(unsafe { crate::ffi::cstr::slice(name) })
}

/// Test `wc` against a class descriptor.
#[unsafe(no_mangle)]
pub extern "C" fn iswctype(wc: u32, kind: u64) -> c_int {
    c_int::from(raven_libc_ctype::iswctype(wc, kind))
}
