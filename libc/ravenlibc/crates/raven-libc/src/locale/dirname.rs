// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dirname` from `<libgen.h>`.

use core::ffi::c_char;

use super::libgen::{DOT, ROOT, writable};

/// `dirname(path)`.
///
/// # Safety
/// `path` is NULL or a NUL-terminated writable string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dirname(path: *mut c_char) -> *mut c_char {
    if path.is_null() {
        return DOT.as_ptr().cast_mut().cast::<c_char>();
    }
    // SAFETY: the caller's buffer contract.
    let bytes = unsafe { writable(path) };
    let mut end = bytes.len();
    while end > 1 && bytes.get(end.wrapping_sub(1)) == Some(&b'/') {
        end = end.wrapping_sub(1);
        if let Some(slot) = bytes.get_mut(end) {
            *slot = 0;
        }
    }
    if end == 0 {
        return DOT.as_ptr().cast_mut().cast::<c_char>();
    }
    let mut slash = end;
    while slash > 0 && bytes.get(slash.wrapping_sub(1)) != Some(&b'/') {
        slash = slash.wrapping_sub(1);
    }
    if slash == 0 {
        return DOT.as_ptr().cast_mut().cast::<c_char>();
    }
    let mut cut = slash;
    while cut > 1 && bytes.get(cut.wrapping_sub(1)) == Some(&b'/') {
        cut = cut.wrapping_sub(1);
    }
    if cut == 1 {
        return ROOT.as_ptr().cast_mut().cast::<c_char>();
    }
    if let Some(slot) = bytes.get_mut(cut) {
        *slot = 0;
    }
    path
}
