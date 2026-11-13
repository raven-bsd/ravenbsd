// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `isxdigit(3)` and `isxdigit_l(3)` (hot: no call).
use core::ffi::c_int;

/// Whether `c` is an ASCII hexadecimal digit.
#[unsafe(no_mangle)]
pub extern "C" fn isxdigit(c: c_int) -> c_int {
    let value = c as u32;
    let digit = value.wrapping_sub(u32::from(b'0')) < 10;
    let lower = (value | 32).wrapping_sub(u32::from(b'a')) < 6;
    c_int::from(digit || lower)
}

/// The `_l` form; the locale is ignored.
#[unsafe(no_mangle)]
pub extern "C" fn isxdigit_l(c: c_int, _locale: *mut core::ffi::c_void) -> c_int {
    let value = c as u32;
    let digit = value.wrapping_sub(u32::from(b'0')) < 10;
    let lower = (value | 32).wrapping_sub(u32::from(b'a')) < 6;
    c_int::from(digit || lower)
}
