// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `str-musl` oracle: `strlen`, `strnlen`, `strchr`, `strrchr`, `__strchrnul`, `__stpcpy`,
//! `__stpncpy`, `strdup`, `strndup`.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::{c_char, c_int};

#[cfg_attr(
    feature = "oracle-str-musl",
    link(name = "oracle_str_musl", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_strlen(s: *const c_char) -> usize;
    pub fn oracle_strnlen(s: *const c_char, n: usize) -> usize;
    pub fn oracle_strchr(s: *const c_char, c: c_int) -> *mut c_char;
    pub fn oracle_strrchr(s: *const c_char, c: c_int) -> *mut c_char;
    pub fn oracle_strchrnul(s: *const c_char, c: c_int) -> *mut c_char;
    pub fn oracle___stpcpy(dst: *mut c_char, src: *const c_char) -> *mut c_char;
    pub fn oracle___stpncpy(dst: *mut c_char, src: *const c_char, n: usize) -> *mut c_char;
    pub fn oracle_strdup(s: *const c_char) -> *mut c_char;
    pub fn oracle_strndup(s: *const c_char, n: usize) -> *mut c_char;
}

unsafe extern "C" {
    /// Host `free`, used to release the donor's `strdup` result.
    pub fn free(ptr: *mut core::ffi::c_void);
}
