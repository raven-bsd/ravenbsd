// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `str-mem` oracle: musl `memcpy`, `memmove`, `memset`, `memcmp`, `memchr`.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::{c_int, c_void};

#[cfg_attr(
    feature = "oracle-str-mem",
    link(name = "oracle_str_mem", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_memcpy(dst: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
    pub fn oracle_memmove(dst: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
    pub fn oracle_memset(dst: *mut c_void, c: c_int, n: usize) -> *mut c_void;
    pub fn oracle_memcmp(a: *const c_void, b: *const c_void, n: usize) -> c_int;
    pub fn oracle_memchr(s: *const c_void, c: c_int, n: usize) -> *mut c_void;
}
