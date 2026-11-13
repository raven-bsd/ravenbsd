// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! C declarations of the `compat-crypt` oracle (bcrypt/blowfish + the three musl crypt formats).
//!
//! The recipe builds; the differential test needs the C35 feature/decaration wiring and the
//! `crypt_private` visibility, so it is deferred. The C65 crate carries the vectors.

#![allow(dead_code)]

use core::ffi::c_char;

unsafe extern "C" {
    pub fn oracle___bcrypt(key: *const c_char, salt: *const c_char) -> *mut c_char;
    pub fn oracle_bcrypt_gensalt(log_rounds: u8) -> *mut c_char;
    pub fn oracle___crypt_md5(key: *const c_char, setting: *const c_char, output: *mut c_char) -> *mut c_char;
    pub fn oracle___crypt_sha256(key: *const c_char, setting: *const c_char, output: *mut c_char) -> *mut c_char;
    pub fn oracle___crypt_sha512(key: *const c_char, setting: *const c_char, output: *mut c_char) -> *mut c_char;
}
