// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wcwidth(3)`.
use core::ffi::c_int;

/// The column width of `wc`.
#[unsafe(no_mangle)]
pub extern "C" fn wcwidth(wc: u32) -> c_int {
    raven_libc_ctype::wcwidth(wc)
}
