// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `isascii(3)` and `toascii(3)`.
use core::ffi::c_int;

/// Whether `c` is a 7-bit value.
#[unsafe(no_mangle)]
pub extern "C" fn isascii(c: c_int) -> c_int {
    c_int::from((0..=0x7f).contains(&c))
}

/// The low 7 bits of `c`.
#[unsafe(no_mangle)]
pub extern "C" fn toascii(c: c_int) -> c_int {
    c & 0x7f
}
