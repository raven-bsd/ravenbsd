// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `bcopy(3)`, `bzero(3)`, `index(3)` and `rindex(3)`.
use core::ffi::{c_char, c_int, c_void};

/// The pre-ANSI overlap-tolerant copy.
///
/// # Safety
/// `src` has `n` readable bytes and `dest` `n` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bcopy(src: *const c_void, dest: *mut c_void, n: usize) {
    // SAFETY: the caller's contract is the bcopy(3) contract.
    unsafe { raven_libc_str::raw::bcopy(src.cast(), dest.cast(), n) };
}

/// The pre-ANSI zero fill.
///
/// # Safety
/// `p` has `n` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bzero(p: *mut c_void, n: usize) {
    // SAFETY: the caller's contract is the bzero(3) contract.
    unsafe { raven_libc_str::raw::bzero(p.cast(), n) };
}

/// The pre-ANSI `strchr`.
///
/// # Safety
/// `s` is a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn index(s: *const c_char, c: c_int) -> *mut c_char {
    // SAFETY: the caller's contract is the index(3) contract.
    unsafe { raven_libc_str::raw::index(s.cast(), c).cast() }
}

/// The pre-ANSI `strrchr`.
///
/// # Safety
/// `s` is a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rindex(s: *const c_char, c: c_int) -> *mut c_char {
    // SAFETY: the caller's contract is the rindex(3) contract.
    unsafe { raven_libc_str::raw::rindex(s.cast(), c).cast() }
}
