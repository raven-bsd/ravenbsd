// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strlen(3)`.
use core::ffi::c_char;

/// The length of the NUL-terminated string `s`.
///
/// # Safety
/// `s` points to a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strlen(s: *const c_char) -> usize {
    let mut length = 0usize;
    loop {
        // SAFETY: the C string contract guarantees a NUL before any unreadable byte.
        if unsafe { *s.cast::<u8>().add(length) } == 0 {
            return length;
        }
        length = length.wrapping_add(1);
    }
}
