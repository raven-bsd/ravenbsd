// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `tolower(3)` and `tolower_l(3)` (hot: no call).
use core::ffi::c_int;

/// Map an ASCII upper-case letter to lower case.
#[unsafe(no_mangle)]
pub extern "C" fn tolower(c: c_int) -> c_int {
    if (c as u32).wrapping_sub(u32::from(b'A')) < 26 {
        c | 0x20
    } else {
        c
    }
}

/// The `_l` form; the locale is ignored.
#[unsafe(no_mangle)]
pub extern "C" fn tolower_l(c: c_int, _locale: *mut core::ffi::c_void) -> c_int {
    if (c as u32).wrapping_sub(u32::from(b'A')) < 26 {
        c | 0x20
    } else {
        c
    }
}
