// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `btowc(3)` and `wctob(3)`.
use core::ffi::c_int;

use raven_libc_ctype::MbMode;

/// The wide character for the single byte `c`.
#[unsafe(no_mangle)]
pub extern "C" fn btowc(c: c_int) -> u32 {
    raven_libc_ctype::btowc(MbMode::Utf8, c)
}

/// The single byte for `c`, or `EOF`.
#[unsafe(no_mangle)]
pub extern "C" fn wctob(c: u32) -> c_int {
    raven_libc_ctype::wctob(MbMode::Utf8, c)
}
