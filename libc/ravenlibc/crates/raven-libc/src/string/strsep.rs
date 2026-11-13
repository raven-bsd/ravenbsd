// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strsep(3)`.
use core::ffi::c_char;

/// Split at the first delimiter, advancing the caller's string pointer.
///
/// # Safety
/// `stringp` is a writable pointer to a NUL-terminated string; `delim` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strsep(stringp: *mut *mut c_char, delim: *const c_char) -> *mut c_char {
    // SAFETY: the caller's contract is the strsep(3) contract.
    unsafe { raven_libc_str::raw::strsep(stringp.cast(), delim.cast()).cast() }
}
