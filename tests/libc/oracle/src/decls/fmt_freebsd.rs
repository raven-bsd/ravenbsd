// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `fmt-freebsd` oracle: the FreeBSD `snprintf`/`swprintf` variadic entries.
//!
//! The `v*` forms take a `va_list` that stable Rust cannot build, so the tests
//! call the variadic public entry points; the C35 shape dispatch supplies the
//! concrete argument list.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::{c_char, c_int};

#[cfg_attr(
    feature = "oracle-fmt-freebsd",
    link(name = "oracle_fmt_freebsd", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_snprintf(s: *mut c_char, n: usize, fmt: *const c_char, ...) -> c_int;
    pub fn oracle_swprintf(s: *mut i32, n: usize, fmt: *const i32, ...) -> c_int;
}
