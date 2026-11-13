// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wctrans(3)`, `towctrans(3)`, `towupper(3)` and `towlower(3)`.
use core::ffi::c_char;

/// Resolve a mapping name to its `wctrans_t`.
///
/// # Safety
/// `name` is a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wctrans(name: *const c_char) -> u64 {
    // SAFETY: the caller's string contract.
    raven_libc_ctype::wctrans(unsafe { crate::ffi::cstr::slice(name) })
}

/// Apply a mapping descriptor.
#[unsafe(no_mangle)]
pub extern "C" fn towctrans(wc: u32, mapping: u64) -> u32 {
    raven_libc_ctype::towctrans(wc, mapping)
}

/// The upper-case mapping.
#[unsafe(no_mangle)]
pub extern "C" fn towupper(wc: u32) -> u32 {
    raven_libc_ctype::towupper(wc)
}

/// The lower-case mapping.
#[unsafe(no_mangle)]
pub extern "C" fn towlower(wc: u32) -> u32 {
    raven_libc_ctype::towlower(wc)
}
