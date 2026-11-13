// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `iswprint(3)`.
use core::ffi::c_int;

/// Whether `wc` is printable.
#[unsafe(no_mangle)]
pub extern "C" fn iswprint(wc: u32) -> c_int {
    c_int::from(raven_libc_ctype::iswprint(wc))
}
