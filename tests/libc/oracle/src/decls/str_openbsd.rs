// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `str-openbsd` oracle: the OpenBSD string family used by the str crate.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::{c_char, c_int, c_void};

#[cfg_attr(
    feature = "oracle-str-openbsd",
    link(name = "oracle_str_openbsd", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_strcmp(a: *const c_char, b: *const c_char) -> c_int;
    pub fn oracle_strncmp(a: *const c_char, b: *const c_char, n: usize) -> c_int;
    pub fn oracle_strcasecmp(a: *const c_char, b: *const c_char) -> c_int;
    pub fn oracle_strncasecmp(a: *const c_char, b: *const c_char, n: usize) -> c_int;
    pub fn oracle_strlcpy(dst: *mut c_char, src: *const c_char, dsize: usize) -> usize;
    pub fn oracle_strlcat(dst: *mut c_char, src: *const c_char, dsize: usize) -> usize;
    pub fn oracle_strncat(dst: *mut c_char, src: *const c_char, n: usize) -> *mut c_char;
    pub fn oracle_memrchr(s: *const c_void, c: c_int, n: usize) -> *mut c_void;
    pub fn oracle_strstr(h: *const c_char, n: *const c_char) -> *mut c_char;
    pub fn oracle_strcasestr(h: *const c_char, n: *const c_char) -> *mut c_char;
    pub fn oracle_timingsafe_bcmp(a: *const c_void, b: *const c_void, n: usize) -> c_int;
    pub fn oracle_timingsafe_memcmp(a: *const c_void, b: *const c_void, n: usize) -> c_int;
    pub fn oracle_strmode(mode: u32, out: *mut c_char);
    pub fn oracle_swab(from: *const c_void, to: *mut c_void, len: isize);
}
