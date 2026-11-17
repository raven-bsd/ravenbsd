// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `basename` from `<libgen.h>`.

use core::ffi::c_char;

use super::libgen::{DOT, writable};

/// `basename(path)`.
///
/// # Safety
/// `path` is NULL or a NUL-terminated writable string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn basename(path: *mut c_char) -> *mut c_char {
    if path.is_null() {
        return DOT.as_ptr().cast_mut().cast::<c_char>();
    }
    // SAFETY: the caller's buffer contract.
    let bytes = unsafe { writable(path) };
    if bytes.is_empty() {
        return DOT.as_ptr().cast_mut().cast::<c_char>();
    }
    let mut end = bytes.len();
    while end > 1 && bytes.get(end.wrapping_sub(1)) == Some(&b'/') {
        end = end.wrapping_sub(1);
        if let Some(slot) = bytes.get_mut(end) {
            *slot = 0;
        }
    }
    if end == 1 && bytes.first() == Some(&b'/') {
        return path;
    }
    let mut start = end;
    while start > 0 && bytes.get(start.wrapping_sub(1)) != Some(&b'/') {
        start = start.wrapping_sub(1);
    }
    // SAFETY: `start <= len`, so the offset stays inside the caller's allocation.
    unsafe { path.add(start) }
}
