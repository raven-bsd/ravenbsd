// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `isblank(3)` and `isblank_l(3)`.
use core::ffi::c_int;

/// Whether `c` is a horizontal blank.
#[unsafe(no_mangle)]
pub extern "C" fn isblank(c: c_int) -> c_int {
    c_int::from(matches!(c, 0x20 | 0x09))
}

/// The `_l` form; the locale is ignored.
#[unsafe(no_mangle)]
pub extern "C" fn isblank_l(c: c_int, _locale: *mut core::ffi::c_void) -> c_int {
    c_int::from(matches!(c, 0x20 | 0x09))
}
