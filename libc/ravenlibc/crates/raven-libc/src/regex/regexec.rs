// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `regexec(3)`.
use core::ffi::{c_char, c_int};

use super::{RegMatchC, RegexT, ShellLocale, load};

/// Match `string` against a compiled expression.
///
/// # Safety
/// `preg` is a valid `regex_t`; `string` is NUL terminated; `pmatch` is NULL or points to
/// `nmatch` writable `regmatch_t` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn regexec(
    preg: *const RegexT,
    string: *const c_char,
    nmatch: usize,
    pmatch: *mut RegMatchC,
    eflags: c_int,
) -> c_int {
    // SAFETY: the caller's regex_t.
    let Some(re) = (unsafe { load(preg.cast::<usize>()) }) else {
        return raven_libc_regex::consts::REG_INVARG;
    };
    // SAFETY: the caller's string and match-array contracts.
    let bytes = unsafe { crate::ffi::cstr::slice(string) };
    // SAFETY: as above; `nmatch` entries are readable when `pmatch` is non-NULL.
    let mut matches = unsafe { super::copy_matches(pmatch, nmatch) };
    let result =
        raven_libc_regex::regexec(re, bytes, &mut matches, super::eflags(eflags), &ShellLocale);
    // SAFETY: the same array the input was read from; `matches.len() == nmatch`.
    unsafe { super::write_matches(pmatch, &matches) };
    match result {
        Ok(()) => 0,
        Err(error) => error.0,
    }
}
