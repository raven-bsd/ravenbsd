// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `regerror(3)`.
use core::ffi::{c_char, c_int};

use super::{RegexT, load};

/// Return the message for `code`, copying it into `buf` when it is non-empty.
///
/// # Safety
/// `preg` is a valid `regex_t` or NULL; `buf` is NULL or points to `size` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn regerror(
    code: c_int,
    preg: *const RegexT,
    buf: *mut c_char,
    size: usize,
) -> usize {
    // SAFETY: the caller's regex_t and buffer contracts.
    let re = unsafe { load(preg.cast::<usize>()) };
    // SAFETY: as above.
    let out = unsafe { crate::ffi::cstr::slice_mut(buf, size) };
    raven_libc_regex::regerror(super::error_code(code), re, out)
}
