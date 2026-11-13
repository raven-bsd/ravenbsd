// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strcpy(3)`, `strncpy(3)`, `stpcpy(3)` and `stpncpy(3)`.
use core::ffi::c_char;

/// Copy `src` including the NUL.
///
/// # Safety
/// `dst` has `strlen(src) + 1` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcpy(dst: *mut c_char, src: *const c_char) -> *mut c_char {
    // SAFETY: the caller's contract is the strcpy(3) contract.
    unsafe { raven_libc_str::raw::strcpy(dst.cast(), src.cast()).cast() }
}

/// Copy at most `n` bytes, zero-filling the rest of `dst[..n]`.
///
/// # Safety
/// `dst` has `n` writable bytes and `src` is a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncpy(dst: *mut c_char, src: *const c_char, n: usize) -> *mut c_char {
    // SAFETY: the caller's contract is the strncpy(3) contract.
    unsafe { raven_libc_str::raw::strncpy(dst.cast(), src.cast(), n).cast() }
}

/// Copy `src` and return the address of the terminating NUL.
///
/// # Safety
/// `dst` has `strlen(src) + 1` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stpcpy(dst: *mut c_char, src: *const c_char) -> *mut c_char {
    // SAFETY: the caller's contract is the stpcpy(3) contract.
    unsafe { raven_libc_str::raw::stpcpy(dst.cast(), src.cast()).cast() }
}

/// `stpncpy`: copy at most `n` bytes and return the NUL (or `dst + n`).
///
/// # Safety
/// `dst` has `n` writable bytes and `src` is a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stpncpy(dst: *mut c_char, src: *const c_char, n: usize) -> *mut c_char {
    // SAFETY: the caller's contract is the stpncpy(3) contract.
    unsafe { raven_libc_str::raw::stpncpy(dst.cast(), src.cast(), n).cast() }
}
