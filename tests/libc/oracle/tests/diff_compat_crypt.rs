// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `compat-crypt`: the Rust `crypt_r` hash formats against the donor C
//! (`bcrypt` from NetBSD, the `$1$`/`$5$`/`$6$` formats from musl).

#![cfg(feature = "oracle-compat-crypt")]
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::undocumented_unsafe_blocks
)]

use core::ffi::c_char;
use std::ffi::{CStr, CString};

use raven_libc_compat::crypt::crypt_r;
use raven_libc_oracle::decls::compat_crypt::{
    oracle___bcrypt, oracle___crypt_md5, oracle___crypt_sha256, oracle___crypt_sha512,
};

fn subject_hash(key: &[u8], salt: &[u8]) -> Vec<u8> {
    let mut out = [0u8; 256];
    let length = crypt_r(key, salt, &mut out).expect("crypt_r");
    out[..length].to_vec()
}

type CryptFn = unsafe extern "C" fn(*const c_char, *const c_char, *mut c_char) -> *mut c_char;

fn oracle_crypt(function: CryptFn, key: &str, setting: &str) -> Vec<u8> {
    let key = CString::new(key).expect("key");
    let setting = CString::new(setting).expect("setting");
    let mut out = [0u8; 256];
    let result = unsafe {
        function(
            key.as_ptr(),
            setting.as_ptr(),
            out.as_mut_ptr().cast::<c_char>(),
        )
    };
    assert!(!result.is_null());
    unsafe { CStr::from_ptr(result) }.to_bytes().to_vec()
}

#[test]
fn crypt_formats_match_the_donor() {
    let md5 = oracle_crypt(oracle___crypt_md5, "0.s0.l33t", "$1$deadbeef$");
    assert_eq!(md5, subject_hash(b"0.s0.l33t", b"$1$deadbeef$"));

    let sha256 = oracle_crypt(oracle___crypt_sha256, "Hello world!", "$5$saltstring");
    assert_eq!(sha256, subject_hash(b"Hello world!", b"$5$saltstring"));

    let sha512 = oracle_crypt(oracle___crypt_sha512, "Hello world!", "$6$saltstring");
    assert_eq!(sha512, subject_hash(b"Hello world!", b"$6$saltstring"));
}

#[test]
fn bcrypt_matches_the_donor() {
    // The unmodified NetBSD donor accepts `$2a$`; the `$2b$` minor is the subject's ticket
    // extension and is covered by the crate unit tests, not this differential.
    let key = CString::new("password").expect("key");
    let salt = CString::new("$2a$04$abcdefghijklmnopqrstuu").expect("salt");
    let result = unsafe { oracle___bcrypt(key.as_ptr(), salt.as_ptr()) };
    assert!(!result.is_null());
    let oracle = unsafe { CStr::from_ptr(result) }.to_bytes().to_vec();
    assert_eq!(
        oracle,
        subject_hash(b"password", b"$2a$04$abcdefghijklmnopqrstuu")
    );
}
