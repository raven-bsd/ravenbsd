// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strnlen(3)`.
use core::ffi::c_char;

/// The number of bytes before the NUL or `n`, whichever is smaller.
///
/// # Safety
/// `s` points to at least `n` readable bytes or to a NUL before them.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strnlen(s: *const c_char, n: usize) -> usize {
    let mut length = 0usize;
    while length < n {
        // SAFETY: the caller guarantees `n` readable bytes or a NUL inside them.
        if unsafe { *s.cast::<u8>().add(length) } == 0 {
            break;
        }
        length = length.wrapping_add(1);
    }
    length
}
