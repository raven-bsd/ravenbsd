// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strstr(3)`, `strcasestr(3)` and `strnstr(3)`.
use core::ffi::c_char;

/// The first occurrence of the NUL-terminated `needle` in `haystack`.
///
/// # Safety
/// Both pointers are readable NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strstr(haystack: *const c_char, needle: *const c_char) -> *mut c_char {
    // SAFETY: the caller's contract is the strstr(3) contract.
    unsafe { raven_libc_str::raw::strstr(haystack.cast(), needle.cast()).cast() }
}

/// The first case-insensitive occurrence.
///
/// # Safety
/// Both pointers are readable NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcasestr(haystack: *const c_char, needle: *const c_char) -> *mut c_char {
    // SAFETY: the caller's contract is the strcasestr(3) contract.
    unsafe { raven_libc_str::raw::strcasestr(haystack.cast(), needle.cast()).cast() }
}

/// The first occurrence inside the first `len` bytes of `haystack`.
///
/// # Safety
/// `haystack` grants `len` readable bytes and both strings are NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strnstr(
    haystack: *const c_char,
    needle: *const c_char,
    len: usize,
) -> *mut c_char {
    // SAFETY: the caller's contract is the strnstr(3) contract.
    unsafe { raven_libc_str::raw::strnstr(haystack.cast(), needle.cast(), len).cast() }
}
