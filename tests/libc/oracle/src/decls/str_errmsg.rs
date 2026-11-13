// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `str-errmsg` oracle: musl `strerror` and `strsignal`.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::c_char;

#[cfg_attr(
    feature = "oracle-str-errmsg",
    link(name = "oracle_str_errmsg", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_strerror(e: i32) -> *mut c_char;
    pub fn oracle_strsignal(s: i32) -> *mut c_char;
}
