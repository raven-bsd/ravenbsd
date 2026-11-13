// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `str-freebsd` oracle: `strnstr`, `strverscmp`, `mempcpy`, `memset_s`, `fls*` and a stdbit
//! sample (the unit tests cover all 70 stdbit functions exhaustively).

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::{c_char, c_int, c_void};

#[cfg_attr(
    feature = "oracle-str-freebsd",
    link(name = "oracle_str_freebsd", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_strnstr(h: *const c_char, n: *const c_char, len: usize) -> *mut c_char;
    pub fn oracle_strverscmp(a: *const c_char, b: *const c_char) -> c_int;
    pub fn oracle_mempcpy(dst: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
    pub fn oracle_memset_s(dst: *mut c_void, smax: usize, c: c_int, n: usize) -> c_int;
    pub fn oracle_fls(x: c_int) -> c_int;
    pub fn oracle_flsl(x: i64) -> c_int;
    pub fn oracle_flsll(x: i64) -> c_int;
    pub fn oracle_stdc_count_ones_uc(x: u8) -> u32;
    pub fn oracle_stdc_bit_ceil_ui(x: u32) -> u32;
    pub fn oracle_stdc_leading_zeros_ull(x: u64) -> u32;
}
