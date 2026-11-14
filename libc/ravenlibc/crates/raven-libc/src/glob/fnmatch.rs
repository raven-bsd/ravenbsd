// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fnmatch(3)`.
use core::ffi::{c_char, c_int};

use super::{ShellPatternLocale, fnmatch_flags};

/// Match `string` against the pattern `pattern`.
///
/// # Safety
/// Both pointers are NUL-terminated C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fnmatch(
    pattern: *const c_char,
    string: *const c_char,
    flags: c_int,
) -> c_int {
    // SAFETY: the caller's string contracts.
    let pattern = unsafe { crate::ffi::cstr::slice(pattern) };
    // SAFETY: as above.
    let string = unsafe { crate::ffi::cstr::slice(string) };
    raven_libc_glob::fnmatch(pattern, string, fnmatch_flags(flags), &ShellPatternLocale)
}
