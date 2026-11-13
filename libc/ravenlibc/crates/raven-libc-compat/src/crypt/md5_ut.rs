// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! MD5 and MD5-crypt tests: the RFC 1321 vectors (`md5sum`, GNU coreutils) and the donor's own
//! self-test, cross-checked with `openssl passwd -1`.

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

fn hex(bytes: &[u8]) -> alloc::string::String {
    use core::fmt::Write;
    let mut out = alloc::string::String::new();
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[test]
fn rfc1321_vectors() {
    let rows: &[(&[u8], &str)] = &[
        (b"", "d41d8cd98f00b204e9800998ecf8427e"),
        (b"a", "0cc175b9c0f1b6a831c399e269772661"),
        (b"abc", "900150983cd24fb0d6963f7d28e17f72"),
        (b"message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
        (
            b"abcdefghijklmnopqrstuvwxyz",
            "c3fcd3d76192e4007dfb496cca67e13b",
        ),
        (
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
            "d174ab98d277d9f5a5611c2c9f419d9f",
        ),
        (
            b"12345678901234567890123456789012345678901234567890123456789012345678901234567890",
            "57edf4a22be3c955ac49da2e2107b67a",
        ),
    ];
    for (input, expected) in rows {
        assert_eq!(hex(&digest(input)), *expected, "input {input:?}");
    }
}

#[test]
fn streaming_matches_one_shot_at_every_split() {
    let input: alloc::vec::Vec<u8> = (0u8..=130).collect();
    let one = digest(&input);
    for split in 0..=input.len() {
        let mut context = Md5::new();
        context.update(&input[..split]);
        context.update(&input[split..]);
        assert_eq!(context.sum(), one, "split {split}");
    }
}

#[test]
fn md5crypt_ticket_anchor() {
    let hash = md5crypt(b"0.s0.l33t", b"$1$deadbeef$0Huu6KHrKLVWfqa4WljDE0").unwrap();
    assert_eq!(hash, b"$1$deadbeef$0Huu6KHrKLVWfqa4WljDE0");
}

#[test]
fn md5crypt_donor_self_test() {
    // openbsd/musl crypt_md5.c's internal test vector.
    let key = b"Xy01@#\x01\x02\x80\x7f\xff\r\n\x81\t !";
    let hash = md5crypt(key, b"$1$abcd0123$").unwrap();
    assert_eq!(hash, b"$1$abcd0123$9Qcg8DyviekV3tDGMZynJ1");
}

#[test]
fn md5crypt_salt_is_cut_at_eight_and_at_dollar() {
    let hash = md5crypt(b"pw", b"$1$1234567890$").unwrap();
    let text = core::str::from_utf8(&hash).unwrap();
    assert!(text.starts_with("$1$12345678$"), "{text}");
    let hash = md5crypt(b"pw", b"$1$ab$rest").unwrap();
    assert!(core::str::from_utf8(&hash).unwrap().starts_with("$1$ab$"));
}

#[test]
fn md5crypt_empty_password_and_salt() {
    // "crypt("", "$1$")" is accepted by the donor (empty salt after the magic).
    let hash = md5crypt(b"", b"$1$").unwrap();
    assert!(core::str::from_utf8(&hash).unwrap().starts_with("$1$$"));
}

#[test]
fn md5crypt_rejects_wrong_magic_and_long_keys() {
    assert!(md5crypt(b"pw", b"$6$salt").is_none());
    assert!(md5crypt(b"pw", b"plain").is_none());
    let long = alloc::vec![b'a'; 30001];
    assert!(md5crypt(&long, b"$1$salt").is_none());
}

#[test]
fn md5crypt_output_is_at_most_34_bytes() {
    let hash = md5crypt(b"pw", b"$1$12345678$").unwrap();
    assert!(hash.len() <= 34);
}
