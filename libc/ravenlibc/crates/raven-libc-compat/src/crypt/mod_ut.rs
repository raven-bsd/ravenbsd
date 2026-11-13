// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `crypt_r` dispatch tests: every accepted format, every rejected salt, the output cap and
//! the fixed `crypt_data` shape.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;
use crate::testutil::FakeEntropy;

fn errno(value: i32) -> Errno {
    Errno(value as u16)
}

fn hash(password: &[u8], salt: &[u8]) -> Result<alloc::vec::Vec<u8>, Errno> {
    let mut out = [0u8; 256];
    let length = crypt_r(password, salt, &mut out)?;
    assert_eq!(out[length], 0);
    Ok(out[..length].to_vec())
}

#[test]
fn md5_format() {
    assert_eq!(
        hash(b"0.s0.l33t", b"$1$deadbeef$").unwrap(),
        b"$1$deadbeef$0Huu6KHrKLVWfqa4WljDE0"
    );
}

#[test]
fn sha256_format() {
    assert_eq!(
        hash(b"Hello world!", b"$5$saltstring").unwrap(),
        b"$5$saltstring$5B8vYYiY.CVt1RlTTf8KbXBH3hsxY/GNooZaBBGWEc5"
    );
}

#[test]
fn sha512_format() {
    assert_eq!(
        hash(b"Hello world!", b"$6$saltstring").unwrap(),
        b"$6$saltstring$svn8UoSVapNtMuq1ukKS4tPQd8iKwSMHWjl/O817G3uBnIFNjnQJuesI68u4OTLiBFdcbYEdFCoEOfaS35inz1"
    );
}

#[test]
fn bcrypt_formats() {
    let result = hash(b"password", b"$2b$04$abcdefghijklmnopqrstuu").unwrap();
    assert_eq!(
        result,
        b"$2b$04$abcdefghijklmnopqrstuuHq1QFV79p.2gtAgWqpJyLiGrJ/Z2Fza"
    );
    let result = hash(b"password", b"$2a$04$abcdefghijklmnopqrstuu").unwrap();
    assert_eq!(
        result,
        b"$2a$04$abcdefghijklmnopqrstuughE8Ev8uGFaUgY2cNEySvxngrb/Jzdm"
    );
}

#[test]
fn rejected_salts() {
    for salt in [
        b"".as_slice(),
        b"ab",
        b"_J9..CCCC",
        b"CCNf8Sbh3HDfQ",
        b"$2y$04$abcdefghijklmnopqrstuu",
        b"$2x$04$abcdefghijklmnopqrstuu",
        b"$3$",
        b"plain-salt",
        b"$z$salt",
    ] {
        assert_eq!(
            hash(b"pw", salt),
            Err(errno(raven_libc_abi::errno::EINVAL)),
            "salt {salt:?}"
        );
    }
}

#[test]
fn empty_salt_after_the_sha_magic_is_accepted() {
    assert!(hash(b"pw", b"$5$").is_ok());
    assert!(hash(b"pw", b"$6$").is_ok());
}

#[test]
fn output_never_exceeds_the_documented_cap() {
    let mut out = [0u8; 256];
    let length = crypt_r(b"x", b"$6$rounds=1000$toolongsaltstring", &mut out).unwrap();
    assert!(length <= CRYPT_HASH_MAX, "length {length}");
    assert_eq!(out[length], 0);
}

#[test]
fn crypt_data_is_260_bytes() {
    assert_eq!(core::mem::size_of::<CryptData>(), 260);
    assert_eq!(core::mem::align_of::<CryptData>(), 4);
}

#[test]
fn gensalt_is_deterministic_with_the_same_entropy() {
    let mut first = FakeEntropy::new(7);
    let mut second = FakeEntropy::new(7);
    let mut a = [0u8; 30];
    let mut b = [0u8; 30];
    assert_eq!(
        bcrypt_gensalt(&mut first, 7, &mut a).unwrap(),
        bcrypt_gensalt(&mut second, 7, &mut b).unwrap()
    );
    assert_eq!(a, b);
}

#[test]
fn md5_with_an_empty_salt_is_accepted_like_the_donor() {
    assert!(hash(b"pw", b"$1$").is_ok());
}
