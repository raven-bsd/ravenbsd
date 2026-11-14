// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `hstrerror(3)`.
use core::ffi::{c_char, c_int};

/// The text of one legacy `h_errno` value.
#[unsafe(no_mangle)]
pub extern "C" fn hstrerror(code: c_int) -> *const c_char {
    crate::ffi::map_net::text_pointer(crate::ffi::map_net::hstrerror_text(code))
}
