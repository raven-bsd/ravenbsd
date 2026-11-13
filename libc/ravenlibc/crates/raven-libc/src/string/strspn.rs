// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strspn(3)`, `strcspn(3)` and `strpbrk(3)`.
use core::ffi::c_char;

/// The length of the prefix of `s` made of bytes in `accept`.
///
/// # Safety
/// Both pointers are readable NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strspn(s: *const c_char, accept: *const c_char) -> usize {
    // SAFETY: the caller's contract is the strspn(3) contract.
    unsafe { raven_libc_str::raw::strspn(s.cast(), accept.cast()) }
}

/// The length of the prefix of `s` with no byte in `reject`.
///
/// # Safety
/// Both pointers are readable NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcspn(s: *const c_char, reject: *const c_char) -> usize {
    // SAFETY: the caller's contract is the strcspn(3) contract.
    unsafe { raven_libc_str::raw::strcspn(s.cast(), reject.cast()) }
}

/// The first byte of `s` that is in `accept`.
///
/// # Safety
/// Both pointers are readable NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strpbrk(s: *const c_char, accept: *const c_char) -> *mut c_char {
    // SAFETY: the caller's contract is the strpbrk(3) contract.
    unsafe { raven_libc_str::raw::strpbrk(s.cast(), accept.cast()).cast() }
}
