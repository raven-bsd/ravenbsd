// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! C string pointers to byte slices. The length walk is the same loop as `strlen`; `CStr` is
//! never used (C60 4.3 item 3).

use core::ffi::c_char;

/// A NUL-terminated C string as a read-only slice.
///
/// # Safety
/// `pointer` is NULL or points to a readable NUL-terminated string.
pub(crate) unsafe fn slice<'a>(pointer: *const c_char) -> &'a [u8] {
    if pointer.is_null() {
        return &[];
    }
    // SAFETY: the caller's contract is the C string contract; the walk stops at the NUL.
    let length = unsafe { length(pointer) };
    // SAFETY: `length` readable bytes precede the NUL the walk found.
    unsafe { core::slice::from_raw_parts(pointer.cast::<u8>(), length) }
}

/// The first `n` bytes, or up to the first NUL.
///
/// # Safety
/// `pointer` is NULL or points to at least `n` readable bytes, or to a NUL before them.
pub(crate) unsafe fn slice_n<'a>(pointer: *const c_char, n: usize) -> &'a [u8] {
    if pointer.is_null() {
        return &[];
    }
    let mut length = 0usize;
    while length < n {
        // SAFETY: the caller guarantees `n` readable bytes or a NUL inside them.
        if unsafe { *pointer.cast::<u8>().add(length) } == 0 {
            break;
        }
        length = length.wrapping_add(1);
    }
    // SAFETY: `length <= n` bytes were read.
    unsafe { core::slice::from_raw_parts(pointer.cast::<u8>(), length) }
}

/// A caller's writable buffer.
///
/// # Safety
/// `pointer` is non-NULL and points to `n` writable bytes.
pub(crate) unsafe fn slice_mut<'a>(pointer: *mut c_char, n: usize) -> &'a mut [u8] {
    if pointer.is_null() || n == 0 {
        // A zero-length region is empty even at a NULL pointer; never `from_raw_parts_mut(null, 0)`.
        return &mut [];
    }
    // SAFETY: the caller promises `n` writable bytes.
    unsafe { core::slice::from_raw_parts_mut(pointer.cast::<u8>(), n) }
}

/// The byte length of a NUL-terminated string.
///
/// # Safety
/// `pointer` points to a readable NUL-terminated string.
unsafe fn length(pointer: *const c_char) -> usize {
    let mut length = 0usize;
    loop {
        // SAFETY: the caller's string has a NUL; the walk cannot pass it.
        if unsafe { *pointer.add(length) } == 0 {
            return length;
        }
        length = length.wrapping_add(1);
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "cstr_ut.rs"]
mod tests;
