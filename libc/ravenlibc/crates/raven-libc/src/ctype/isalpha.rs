// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `isalpha(3)` and `isalpha_l(3)` (hot: no call).
use core::ffi::c_int;

/// Whether `c` is an ASCII letter.
#[unsafe(no_mangle)]
pub extern "C" fn isalpha(c: c_int) -> c_int {
    c_int::from(((c as u32) | 32).wrapping_sub(u32::from(b'a')) < 26)
}

/// The `_l` form; the locale is ignored.
#[unsafe(no_mangle)]
pub extern "C" fn isalpha_l(c: c_int, _locale: *mut core::ffi::c_void) -> c_int {
    c_int::from(((c as u32) | 32).wrapping_sub(u32::from(b'a')) < 26)
}
