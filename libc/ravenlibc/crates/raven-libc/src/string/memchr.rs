// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `memchr(3)` and `memrchr(3)`.
use core::ffi::{c_int, c_void};

/// The first occurrence of `c` in the first `n` bytes of `s`.
///
/// # Safety
/// `s` points to `n` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memchr(s: *const c_void, c: c_int, n: usize) -> *mut c_void {
    let byte = (c as u32 & 0xff) as u8;
    let base = s.cast::<u8>();
    let mut index = 0usize;
    while index < n {
        // SAFETY: the caller grants `n` readable bytes.
        if unsafe { *base.add(index) } == byte {
            // SAFETY: `index < n`, so the pointer is inside the caller's region.
            return unsafe { base.add(index).cast_mut().cast() };
        }
        index = index.wrapping_add(1);
    }
    core::ptr::null_mut()
}

/// The last occurrence of `c` in the first `n` bytes of `s`.
///
/// # Safety
/// `s` points to `n` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memrchr(s: *const c_void, c: c_int, n: usize) -> *mut c_void {
    let byte = (c as u32 & 0xff) as u8;
    let base = s.cast::<u8>();
    let mut index = n;
    while index > 0 {
        index = index.wrapping_sub(1);
        // SAFETY: the caller grants `n` readable bytes and `index < n`.
        if unsafe { *base.add(index) } == byte {
            // SAFETY: `index < n`, so the pointer is inside the caller's region.
            return unsafe { base.add(index).cast_mut().cast() };
        }
    }
    core::ptr::null_mut()
}
