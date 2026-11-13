// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strcmp(3)` and `strncmp(3)`.
use core::ffi::{c_char, c_int};

/// The sign of the first difference between two NUL-terminated strings.
///
/// # Safety
/// Both pointers are readable NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcmp(a: *const c_char, b: *const c_char) -> c_int {
    let left = a.cast::<u8>();
    let right = b.cast::<u8>();
    let mut index = 0usize;
    loop {
        // SAFETY: both strings are NUL terminated; the walk stops at the first NUL.
        let x = unsafe { *left.add(index) };
        // SAFETY: as above.
        let y = unsafe { *right.add(index) };
        if x != y {
            return i32::from(x).wrapping_sub(i32::from(y));
        }
        if x == 0 {
            return 0;
        }
        index = index.wrapping_add(1);
    }
}

/// The same comparison bounded by `n`.
///
/// # Safety
/// Both pointers grant `n` readable bytes or contain a NUL before them.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncmp(a: *const c_char, b: *const c_char, n: usize) -> c_int {
    let left = a.cast::<u8>();
    let right = b.cast::<u8>();
    let mut index = 0usize;
    while index < n {
        // SAFETY: the caller grants `n` readable bytes or a NUL inside them.
        let x = unsafe { *left.add(index) };
        // SAFETY: as above.
        let y = unsafe { *right.add(index) };
        if x != y {
            return i32::from(x).wrapping_sub(i32::from(y));
        }
        if x == 0 {
            return 0;
        }
        index = index.wrapping_add(1);
    }
    0
}
