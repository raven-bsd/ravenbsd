// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `regcomp(3)`.
use core::ffi::{c_char, c_int};

use super::{RegexT, ShellLocale, clear, store};

/// Compile a regular expression into `preg`.
///
/// # Safety
/// `preg` points to a `regex_t` of eight writable words; `pattern` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn regcomp(
    preg: *mut RegexT,
    pattern: *const c_char,
    cflags: c_int,
) -> c_int {
    if preg.is_null() {
        return raven_libc_regex::consts::REG_INVARG;
    }
    // SAFETY: the caller's string contract.
    let bytes = unsafe { crate::ffi::cstr::slice(pattern) };
    let slot = preg.cast::<usize>();
    match raven_libc_regex::regcomp(bytes, super::cflags(cflags), &ShellLocale) {
        // SAFETY: `preg` is the caller's regex_t; `store` writes words 0 and 1.
        Ok(re) => {
            unsafe { store(slot, re) };
            0
        }
        Err(error) => {
            // SAFETY: a failed compile leaves no live expression behind.
            unsafe { clear(slot) };
            error.0
        }
    }
}
