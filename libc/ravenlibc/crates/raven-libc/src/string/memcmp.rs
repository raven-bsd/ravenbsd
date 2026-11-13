// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `memcmp(3)` and `bcmp(3)`.
use core::ffi::{c_int, c_void};

/// The sign of the first difference between two `n`-byte regions.
///
/// # Safety
/// Both pointers grant `n` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(a: *const c_void, b: *const c_void, n: usize) -> c_int {
    let left = a.cast::<u8>();
    let right = b.cast::<u8>();
    let mut index = 0usize;
    while index < n {
        // SAFETY: both regions have `n` readable bytes.
        let x = unsafe { *left.add(index) };
        // SAFETY: as above.
        let y = unsafe { *right.add(index) };
        if x != y {
            return i32::from(x).wrapping_sub(i32::from(y));
        }
        index = index.wrapping_add(1);
    }
    0
}

/// `bcmp` is `memcmp` with only the zero/non-zero result specified.
///
/// # Safety
/// Both pointers grant `n` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bcmp(a: *const c_void, b: *const c_void, n: usize) -> c_int {
    let left = a.cast::<u8>();
    let right = b.cast::<u8>();
    let mut index = 0usize;
    while index < n {
        // SAFETY: both regions have `n` readable bytes.
        if unsafe { *left.add(index) } != unsafe { *right.add(index) } {
            return 1;
        }
        index = index.wrapping_add(1);
    }
    0
}
