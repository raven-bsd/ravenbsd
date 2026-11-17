// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `wcscoll`, `wcscoll_l`, `wcsxfrm` and `wcsxfrm_l`.

use core::ffi::c_void;
use core::ptr;

fn compare(a: *const u32, b: *const u32) -> i32 {
    let mut index = 0usize;
    loop {
        // SAFETY: both arguments are NUL-terminated wide strings.
        let left = unsafe { *a.add(index) };
        let right = unsafe { *b.add(index) };
        if left != right {
            return if left < right { -1 } else { 1 };
        }
        if left == 0 {
            return 0;
        }
        index += 1;
    }
}

fn length(value: *const u32) -> usize {
    let mut index = 0usize;
    // SAFETY: the argument is a NUL-terminated wide string.
    while unsafe { *value.add(index) } != 0 {
        index += 1;
    }
    index
}

/// `wcscoll`: compare two wide strings in the C/UTF-8 code-point order.
///
/// # Safety
/// Both arguments are NUL-terminated wide strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcscoll(a: *const u32, b: *const u32) -> i32 {
    if a.is_null() || b.is_null() {
        return 0;
    }
    compare(a, b)
}

/// `wcscoll_l`: as `wcscoll` (the locale object is accepted and ignored).
///
/// # Safety
/// As `wcscoll`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcscoll_l(a: *const u32, b: *const u32, _locale: *mut c_void) -> i32 {
    // SAFETY: forwarded C contract.
    unsafe { wcscoll(a, b) }
}

/// `wcsxfrm`: copy the collation key (the string itself in the C locale).
///
/// # Safety
/// `src` is NUL-terminated; `dst` has `n` writable units or `n == 0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsxfrm(dst: *mut u32, src: *const u32, n: usize) -> usize {
    if src.is_null() {
        return 0;
    }
    let len = length(src);
    if !dst.is_null() && n > 0 {
        let take = len.min(n - 1);
        // SAFETY: `take <= n - 1` and the source has `len + 1` readable units.
        unsafe {
            ptr::copy_nonoverlapping(src, dst, take);
            *dst.add(take) = 0;
        }
    }
    len
}

/// `wcsxfrm_l`: as `wcsxfrm` (the locale object is accepted and ignored).
///
/// # Safety
/// As `wcsxfrm`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcsxfrm_l(
    dst: *mut u32,
    src: *const u32,
    n: usize,
    _locale: *mut c_void,
) -> usize {
    // SAFETY: forwarded C contract.
    unsafe { wcsxfrm(dst, src, n) }
}
