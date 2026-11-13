// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strdup(3)` and `strndup(3)`.
use core::ffi::c_char;

/// A `malloc`ed copy of `s`.
///
/// # Safety
/// `s` is a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strdup(s: *const c_char) -> *mut c_char {
    // SAFETY: the caller's string contract; the allocator is the C60 malloc export.
    unsafe { raven_libc_str::raw::strdup_with(crate::malloc::abi_alloc, s.cast()).cast() }
}

/// A `malloc`ed copy of at most `n` bytes of `s`.
///
/// # Safety
/// `s` is a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strndup(s: *const c_char, n: usize) -> *mut c_char {
    // SAFETY: the caller's string contract; the allocator is the C60 malloc export.
    unsafe { raven_libc_str::raw::strndup_with(crate::malloc::abi_alloc, s.cast(), n).cast() }
}
