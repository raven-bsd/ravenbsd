// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `iswgraph(3)`.
use core::ffi::c_int;

/// Whether `wc` is printable and not a space.
#[unsafe(no_mangle)]
pub extern "C" fn iswgraph(wc: u32) -> c_int {
    c_int::from(raven_libc_ctype::iswgraph(wc))
}
