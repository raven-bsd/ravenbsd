// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! C declarations of the `compat-humanize` oracle
//! (`freebsd:lib/libutil/{humanize_number,expand_number}.c`).
//!
//! Wiring needs a C35 change (`decls/mod.rs` and the oracle feature list).

#![allow(dead_code)]

use core::ffi::{c_char, c_int};

unsafe extern "C" {
    pub fn oracle_humanize_number(
        buffer: *mut c_char,
        length: usize,
        quotient: i64,
        suffix: *const c_char,
        scale: c_int,
        flags: c_int,
    ) -> c_int;
    pub fn oracle_expand_number(text: *const c_char, number: *mut i64) -> c_int;
    pub fn oracle_expand_unsigned(text: *const c_char, number: *mut u64) -> c_int;
}
