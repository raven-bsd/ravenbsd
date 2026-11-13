// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `iscntrl(3)` and `iscntrl_l(3)`.
use core::ffi::c_int;

/// Whether `c` is an ASCII control byte.
#[unsafe(no_mangle)]
pub extern "C" fn iscntrl(c: c_int) -> c_int {
    c_int::from(matches!(c, 0x00..=0x1f | 0x7f))
}

/// The `_l` form; the locale is ignored.
#[unsafe(no_mangle)]
pub extern "C" fn iscntrl_l(c: c_int, _locale: *mut core::ffi::c_void) -> c_int {
    c_int::from(matches!(c, 0x00..=0x1f | 0x7f))
}
