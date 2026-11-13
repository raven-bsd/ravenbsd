// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `isdigit(3)` and `isdigit_l(3)` (hot: no call).
use core::ffi::c_int;

/// Whether `c` is an ASCII decimal digit.
#[unsafe(no_mangle)]
pub extern "C" fn isdigit(c: c_int) -> c_int {
    c_int::from((c as u32).wrapping_sub(u32::from(b'0')) < 10)
}

/// The `_l` form; the locale is ignored.
#[unsafe(no_mangle)]
pub extern "C" fn isdigit_l(c: c_int, _locale: *mut core::ffi::c_void) -> c_int {
    c_int::from((c as u32).wrapping_sub(u32::from(b'0')) < 10)
}
