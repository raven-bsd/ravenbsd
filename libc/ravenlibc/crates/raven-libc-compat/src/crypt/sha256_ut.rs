// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SHA-256 and SHA-256-crypt tests: the FIPS 180-4 vectors (hashes from GNU coreutils
//! `sha256sum`), the donor's self-test and the ticket's `openssl passwd -5` anchors.

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
fn fips_vectors() {
    let rows: &[(&[u8], &str)] = &[
        (b"", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        (b"abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
        (
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
        (
            b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
        ),
    ];
    for (input, expected) in rows {
        assert_eq!(hex(&digest(input)), *expected, "input {input:?}");
    }
}

#[test]
fn million_a_vector() {
    let input = alloc::vec![b'a'; 1_000_000];
    assert_eq!(
        hex(&digest(&input)),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn streaming_matches_one_shot_at_every_split() {
    let input: alloc::vec::Vec<u8> = (0u8..=130).collect();
    let one = digest(&input);
    for split in 0..=input.len() {
        let mut context = Sha256::new();
        context.update(&input[..split]);
        context.update(&input[split..]);
        assert_eq!(context.sum(), one, "split {split}");
    }
}

#[test]
fn ticket_anchor_hello_world() {
    let hash = sha256crypt(b"Hello world!", b"$5$saltstring").unwrap();
    assert_eq!(
        hash,
        b"$5$saltstring$5B8vYYiY.CVt1RlTTf8KbXBH3hsxY/GNooZaBBGWEc5"
    );
}

#[test]
fn ticket_anchor_rounds_and_truncation() {
    let hash = sha256crypt(b"This is just a test", b"$5$rounds=5000$toolongsaltstring").unwrap();
    assert_eq!(
        hash,
        b"$5$rounds=5000$toolongsaltstrin$Un/5jzAHMgOGZ5.mWJpuVolil07guHPvOW8mGRcvxa5"
    );
    let hash = sha256crypt(b"This is just a test", b"$5$toolongsaltstring").unwrap();
    assert_eq!(
        hash,
        b"$5$toolongsaltstrin$Un/5jzAHMgOGZ5.mWJpuVolil07guHPvOW8mGRcvxa5"
    );
}

#[test]
fn donor_self_test() {
    let key = b"Xy01@#\x01\x02\x80\x7f\xff\r\n\x81\t !";
    let hash = sha256crypt(key, b"$5$rounds=1234$abc0123456789$").unwrap();
    assert_eq!(
        hash,
        b"$5$rounds=1234$abc0123456789$3VfDjPt05VHFn47C/ojFZ6KRPYrOjj1lLbH.dkF3bZ6"
    );
}

#[test]
fn rounds_clamping_and_defaults() {
    assert_eq!(parse_rounds(b"rounds=5000$salt").unwrap().0, 5000);
    assert_eq!(parse_rounds(b"rounds=1$salt").unwrap().0, ROUNDS_MIN);
    assert_eq!(
        parse_rounds(b"rounds=999999999$salt").unwrap().0,
        ROUNDS_MAX
    );
    assert_eq!(
        parse_rounds(b"rounds=1000000000$salt").unwrap().0,
        ROUNDS_MAX
    );
    assert_eq!(parse_rounds(b"salt").unwrap().0, ROUNDS_DEFAULT);
    assert!(parse_rounds(b"rounds=x$salt").is_none());
    assert!(parse_rounds(b"rounds=10salt").is_none());
}

#[test]
fn minimum_rounds_hash() {
    let hash = sha256crypt(b"pw", b"$5$rounds=1000$salt").unwrap();
    assert!(
        core::str::from_utf8(&hash)
            .unwrap()
            .starts_with("$5$rounds=1000$salt$")
    );
}

#[test]
fn salt_is_cut_at_sixteen() {
    let hash = sha256crypt(b"This is just a test", b"$5$toolongsaltstring").unwrap();
    assert_eq!(
        hash,
        b"$5$toolongsaltstrin$Un/5jzAHMgOGZ5.mWJpuVolil07guHPvOW8mGRcvxa5"
    );
}

#[test]
fn empty_salt_after_the_magic_is_mirrored() {
    let hash = sha256crypt(b"pw", b"$5$").unwrap();
    assert!(core::str::from_utf8(&hash).unwrap().starts_with("$5$$"));
}

#[test]
fn malformed_settings_are_rejected() {
    assert!(sha256crypt(b"pw", b"$6$salt").is_none());
    assert!(sha256crypt(b"pw", b"$5$rounds=1x$salt").is_none());
    assert!(sha256crypt(b"pw", b"$5$sa:lt").is_none());
    assert!(sha256crypt(b"pw", b"$5$sa\nlt").is_none());
    let long = alloc::vec![b'a'; 257];
    assert!(sha256crypt(&long, b"$5$salt").is_none());
}
