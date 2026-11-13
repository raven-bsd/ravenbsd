// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `fmt-musl` oracle: musl's `snprintf`/`swprintf` variadic entries.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::{c_char, c_int};

#[cfg_attr(
    feature = "oracle-fmt-musl",
    link(name = "oracle_fmt_musl", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_snprintf(s: *mut c_char, n: usize, fmt: *const c_char, ...) -> c_int;
    pub fn oracle_swprintf(s: *mut i32, n: usize, fmt: *const i32, ...) -> c_int;
}
