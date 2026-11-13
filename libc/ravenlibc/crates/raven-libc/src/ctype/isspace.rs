// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `isspace(3)` and `isspace_l(3)` (hot: no call).
use core::ffi::c_int;

/// Whether `c` is an ASCII white-space byte.
#[unsafe(no_mangle)]
pub extern "C" fn isspace(c: c_int) -> c_int {
    c_int::from(matches!(c, 0x20 | 0x09..=0x0d))
}

/// The `_l` form; the locale is ignored.
#[unsafe(no_mangle)]
pub extern "C" fn isspace_l(c: c_int, _locale: *mut core::ffi::c_void) -> c_int {
    c_int::from(matches!(c, 0x20 | 0x09..=0x0d))
}
