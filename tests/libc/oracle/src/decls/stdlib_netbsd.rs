// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `stdlib-netbsd` oracle: `random`, `srandom`, `initstate`, `setstate` and `rand_r`.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::{c_char, c_int, c_long, c_uint};

#[cfg_attr(
    feature = "oracle-stdlib-netbsd",
    link(name = "oracle_stdlib_netbsd", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_srandom(seed: c_uint);
    pub fn oracle_random() -> c_long;
    pub fn oracle_initstate(seed: c_uint, state: *mut c_char, n: usize) -> *mut c_char;
    pub fn oracle_setstate(state: *mut c_char) -> *mut c_char;
    pub fn oracle_rand_r(seed: *mut c_uint) -> c_int;
}
