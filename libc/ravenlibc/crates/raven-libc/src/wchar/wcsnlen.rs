// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `wcsnlen`, `wcstok`, `wcscasecmp_l` and `wcsncasecmp_l`.

use core::ffi::c_void;

/// `wcsnlen`: the number of units before the NUL, capped at `max`.
///
/// # Safety
/// `value` is readable for `max` units or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsnlen(value: *const u32, max: usize) -> usize {
    if value.is_null() {
        return 0;
    }
    let mut index = 0usize;
    while index < max {
        // SAFETY: the caller grants `max` readable units.
        if unsafe { *value.add(index) } == 0 {
            break;
        }
        index += 1;
    }
    index
}

/// `wcstok`: split a wide string on the wide delimiter set.
///
/// # Safety
/// `saveptr` is writable; `s` is NULL or NUL-terminated; `delim` is NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstok(
    s: *mut u32,
    delim: *const u32,
    saveptr: *mut *mut u32,
) -> *mut u32 {
    if delim.is_null() || saveptr.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the caller grants a readable/writable save slot.
    let mut cursor = if s.is_null() { unsafe { *saveptr } } else { s };
    if cursor.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the delimiter set is NUL-terminated.
    let is_delim = |ch: u32| {
        let mut i = 0usize;
        loop {
            // SAFETY: bounded by the delimiter's NUL terminator.
            let d = unsafe { *delim.add(i) };
            if d == 0 {
                return false;
            }
            if d == ch {
                return true;
            }
            i += 1;
        }
    };
    // SAFETY: `cursor` walks the caller's NUL-terminated string.
    while unsafe { *cursor } != 0 && is_delim(unsafe { *cursor }) {
        cursor = unsafe { cursor.add(1) };
    }
    // SAFETY: as above.
    if unsafe { *cursor } == 0 {
        unsafe { *saveptr = cursor };
        return core::ptr::null_mut();
    }
    let token = cursor;
    loop {
        // SAFETY: `cursor` walks the caller's NUL-terminated string.
        let ch = unsafe { *cursor };
        if ch == 0 {
            // SAFETY: the save slot is writable.
            unsafe { *saveptr = cursor };
            break;
        }
        if is_delim(ch) {
            // SAFETY: the save slot and the string are writable.
            unsafe {
                *cursor = 0;
                *saveptr = cursor.add(1);
            }
            break;
        }
        cursor = unsafe { cursor.add(1) };
    }
    token
}

fn compare_case(a: *const u32, b: *const u32, limit: usize) -> i32 {
    let mut index = 0usize;
    while index < limit {
        // SAFETY: both arguments are NUL-terminated wide strings.
        let left = unsafe { *a.add(index) };
        let right = unsafe { *b.add(index) };
        let left_fold = raven_libc_ctype::towlower(left);
        let right_fold = raven_libc_ctype::towlower(right);
        if left_fold != right_fold {
            return if left_fold < right_fold { -1 } else { 1 };
        }
        if left == 0 {
            return 0;
        }
        index += 1;
    }
    0
}

/// `wcscasecmp_l`: case-insensitive wide comparison (locale object ignored).
///
/// # Safety
/// Both arguments are NUL-terminated wide strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcscasecmp_l(a: *const u32, b: *const u32, _locale: *mut c_void) -> i32 {
    if a.is_null() || b.is_null() {
        return 0;
    }
    compare_case(a, b, usize::MAX)
}

/// `wcsncasecmp_l`: case-insensitive wide comparison of at most `n` units.
///
/// # Safety
/// Both arguments are readable for `n` units or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsncasecmp_l(
    a: *const u32,
    b: *const u32,
    n: usize,
    _locale: *mut c_void,
) -> i32 {
    if a.is_null() || b.is_null() {
        return 0;
    }
    compare_case(a, b, n)
}
