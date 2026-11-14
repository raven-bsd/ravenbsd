// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `gai_strerror(3)`.
use core::ffi::{c_char, c_int};

/// The musl text of one `EAI_*` code.
#[unsafe(no_mangle)]
pub extern "C" fn gai_strerror(code: c_int) -> *const c_char {
    crate::ffi::map_net::text_pointer(crate::ffi::map_net::gai_text(code))
}
