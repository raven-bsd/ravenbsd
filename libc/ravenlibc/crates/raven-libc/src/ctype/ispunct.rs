// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ispunct(3)` and `ispunct_l(3)`.
use core::ffi::c_int;

/// Whether `c` is a printable non-alphanumeric non-space byte.
#[unsafe(no_mangle)]
pub extern "C" fn ispunct(c: c_int) -> c_int {
    let printable = matches!(c, 0x21..=0x7e);
    let alnum = {
        let value = c as u32;
        (value | 32).wrapping_sub(u32::from(b'a')) < 26 || value.wrapping_sub(u32::from(b'0')) < 10
    };
    c_int::from(printable && !alnum)
}

/// The `_l` form; the locale is ignored.
#[unsafe(no_mangle)]
pub extern "C" fn ispunct_l(c: c_int, _locale: *mut core::ffi::c_void) -> c_int {
    let printable = matches!(c, 0x21..=0x7e);
    let alnum = {
        let value = c as u32;
        (value | 32).wrapping_sub(u32::from(b'a')) < 26 || value.wrapping_sub(u32::from(b'0')) < 10
    };
    c_int::from(printable && !alnum)
}
