// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `isgraph(3)` and `isgraph_l(3)`.
use core::ffi::c_int;

/// Whether `c` is a printable non-space byte.
#[unsafe(no_mangle)]
pub extern "C" fn isgraph(c: c_int) -> c_int {
    c_int::from(matches!(c, 0x21..=0x7e))
}

/// The `_l` form; the locale is ignored.
#[unsafe(no_mangle)]
pub extern "C" fn isgraph_l(c: c_int, _locale: *mut core::ffi::c_void) -> c_int {
    c_int::from(matches!(c, 0x21..=0x7e))
}
