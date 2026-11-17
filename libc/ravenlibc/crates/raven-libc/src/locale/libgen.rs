// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `basename(3)` and `dirname(3)` from `<libgen.h>`.
//!
//! Both functions may modify the caller's buffer, as the standard allows. They return a
//! pointer into that buffer or one of the two static fallbacks `.` and `/`.

use core::ffi::c_char;

/// The `.` fallback.
static DOT: [u8; 2] = *b".\0";
/// The `/` fallback.
static ROOT: [u8; 2] = *b"/\0";

/// The writable bytes of a NUL-terminated string.
///
/// # Safety
/// `s` is a NUL-terminated writable buffer.
unsafe fn writable<'a>(s: *mut c_char) -> &'a mut [u8] {
    // SAFETY: the caller's C string contract.
    let length = unsafe { crate::ffi::cstr::slice(s) }.len();
    // SAFETY: the same buffer is writable for `length` bytes.
    unsafe { core::slice::from_raw_parts_mut(s.cast::<u8>(), length) }
}

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
