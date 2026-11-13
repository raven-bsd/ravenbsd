// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strchr(3)`, `strrchr(3)` and `strchrnul(3)`.
use core::ffi::{c_char, c_int};

/// The first occurrence of `c` (including the NUL) in `s`.
///
/// # Safety
/// `s` points to a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strchr(s: *const c_char, c: c_int) -> *mut c_char {
    let byte = (c as u32 & 0xff) as u8;
    let base = s.cast::<u8>();
    let mut index = 0usize;
    loop {
        // SAFETY: the C string contract guarantees a NUL.
        let value = unsafe { *base.add(index) };
        if value == byte {
            // SAFETY: `index` is inside the caller's string.
            return unsafe { base.add(index).cast_mut().cast() };
        }
        if value == 0 {
            return core::ptr::null_mut();
        }
        index = index.wrapping_add(1);
    }
}

/// The last occurrence of `c` (the NUL included) in `s`.
///
/// # Safety
/// `s` points to a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strrchr(s: *const c_char, c: c_int) -> *mut c_char {
    let byte = (c as u32 & 0xff) as u8;
    let base = s.cast::<u8>();
    let mut index = 0usize;
    let mut found: *mut u8 = core::ptr::null_mut();
    loop {
        // SAFETY: the C string contract guarantees a NUL.
        let value = unsafe { *base.add(index) };
        if value == byte {
            // SAFETY: `index` is inside the caller's string.
            found = unsafe { base.add(index).cast_mut() };
        }
        if value == 0 {
            return found.cast();
        }
        index = index.wrapping_add(1);
    }
}

/// `strchr` without the not-found NULL: returns the NUL position instead.
///
/// # Safety
/// `s` points to a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strchrnul(s: *const c_char, c: c_int) -> *mut c_char {
    let byte = (c as u32 & 0xff) as u8;
    let base = s.cast::<u8>();
    let mut index = 0usize;
    loop {
        // SAFETY: the C string contract guarantees a NUL.
        let value = unsafe { *base.add(index) };
        if value == byte || value == 0 {
            // SAFETY: `index` is inside the caller's string.
            return unsafe { base.add(index).cast_mut().cast() };
        }
        index = index.wrapping_add(1);
    }
}
