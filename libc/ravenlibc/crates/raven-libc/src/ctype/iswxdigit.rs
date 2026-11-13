// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `iswxdigit(3)`.
use core::ffi::c_int;

/// Whether `wc` is a hexadecimal digit.
#[unsafe(no_mangle)]
pub extern "C" fn iswxdigit(wc: u32) -> c_int {
    c_int::from(raven_libc_ctype::iswxdigit(wc))
}
