// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `toupper(3)` and `toupper_l(3)` (hot: no call).
use core::ffi::c_int;

/// Map an ASCII lower-case letter to upper case.
#[unsafe(no_mangle)]
pub extern "C" fn toupper(c: c_int) -> c_int {
    if (c as u32).wrapping_sub(u32::from(b'a')) < 26 {
        c & 0x5f
    } else {
        c
    }
}

/// The `_l` form; the locale is ignored.
#[unsafe(no_mangle)]
pub extern "C" fn toupper_l(c: c_int, _locale: *mut core::ffi::c_void) -> c_int {
    if (c as u32).wrapping_sub(u32::from(b'a')) < 26 {
        c & 0x5f
    } else {
        c
    }
}
