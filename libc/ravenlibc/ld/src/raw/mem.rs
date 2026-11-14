// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The loader's `mem*` implementations, with plain word loops (A-ld-3).

/// `memcpy`.
///
/// # Safety
/// `dst` and `src` are valid for `len` bytes and do not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dst: *mut u8, src: *const u8, len: usize) -> *mut u8 {
    let mut index = 0usize;
    while index < len {
        // SAFETY: the caller guarantees both ranges.
        unsafe {
            *dst.add(index) = *src.add(index);
        }
        index = index.wrapping_add(1);
    }
    dst
}

/// `memmove`.
///
/// # Safety
/// `dst` and `src` are valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dst: *mut u8, src: *const u8, len: usize) -> *mut u8 {
    if (dst as usize) <= (src as usize) {
        // SAFETY: the caller guarantees both ranges; the forward copy is the non-overlapping
        // form and the condition above orders the overlap.
        return unsafe { memcpy(dst, src, len) };
    }
    let mut index = len;
    while index > 0 {
        index = index.wrapping_sub(1);
        // SAFETY: the caller guarantees both ranges; the backward copy handles overlap.
        unsafe {
            *dst.add(index) = *src.add(index);
        }
    }
    dst
}

/// `memset`.
///
/// # Safety
/// `dst` is valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dst: *mut u8, value: i32, len: usize) -> *mut u8 {
    let byte = value as u8;
    let mut index = 0usize;
    while index < len {
        // SAFETY: the caller guarantees the range.
        unsafe {
            *dst.add(index) = byte;
        }
        index = index.wrapping_add(1);
    }
    dst
}

/// `memcmp`.
///
/// # Safety
/// Both pointers are valid for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(left: *const u8, right: *const u8, len: usize) -> i32 {
    let mut index = 0usize;
    while index < len {
        // SAFETY: the caller guarantees both ranges.
        // SAFETY: the caller guarantees both ranges for `len` bytes.
        let a = unsafe { *left.add(index) };
        // SAFETY: see above.
        let b = unsafe { *right.add(index) };
        if a != b {
            return i32::from(a).wrapping_sub(i32::from(b));
        }
        index = index.wrapping_add(1);
    }
    0
}

/// `bcmp`.
///
/// # Safety
/// See [`memcmp`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bcmp(left: *const u8, right: *const u8, len: usize) -> i32 {
    // SAFETY: the caller's contract is exactly `memcmp`'s.
    unsafe { memcmp(left, right, len) }
}

/// `strlen`.
///
/// # Safety
/// `text` is NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strlen(text: *const u8) -> usize {
    let mut index = 0usize;
    loop {
        // SAFETY: the caller guarantees a NUL-terminated string.
        if unsafe { *text.add(index) } == 0 {
            return index;
        }
        index = index.wrapping_add(1);
    }
}
