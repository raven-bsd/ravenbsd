// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SHA-512 and SHA-512-crypt tests: the FIPS 180-4 vectors (hashes from GNU coreutils
//! `sha512sum`), the donor's self-test and the ticket's `openssl passwd -6` anchors.

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
        (
            b"",
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
        ),
        (
            b"abc",
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
        ),
        (
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "204a8fc6dda82f0a0ced7beb8e08a41657c16ef468b228a8279be331a703c33596fd15c13b1b07f9aa1d3bea57789ca031ad85c7a71dd70354ec631238ca3445",
        ),
        (
            b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
            "8e959b75dae313da8cf4f72814fc143f8f7779c6eb9f7fa17299aeadb6889018501d289e4900f7e4331b99dec4b5433ac7d329eeb6dd26545e96e55b874be909",
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
        "e718483d0ce769644e2e42c7bc15b4638e1f98b13b2044285632a803afa973ebde0ff244877ea60a4cb0432ce577c31beb009c5c2c49aa2e4eadb217ad8cc09b"
    );
}

#[test]
fn streaming_matches_one_shot_at_every_split() {
    let input: alloc::vec::Vec<u8> = (0u8..=130).collect();
    let one = digest(&input);
    for split in 0..=input.len() {
        let mut context = Sha512::new();
        context.update(&input[..split]);
        context.update(&input[split..]);
        assert_eq!(context.sum(), one, "split {split}");
    }
}

#[test]
fn ticket_anchor_hello_world() {
    let hash = sha512crypt(b"Hello world!", b"$6$saltstring").unwrap();
    assert_eq!(
        hash,
        b"$6$saltstring$svn8UoSVapNtMuq1ukKS4tPQd8iKwSMHWjl/O817G3uBnIFNjnQJuesI68u4OTLiBFdcbYEdFCoEOfaS35inz1"
    );
}

#[test]
fn ticket_anchor_rounds_and_truncation() {
    let hash = sha512crypt(b"Hello world!", b"$6$saltstring").unwrap();
    assert_eq!(
        hash,
        b"$6$saltstring$svn8UoSVapNtMuq1ukKS4tPQd8iKwSMHWjl/O817G3uBnIFNjnQJuesI68u4OTLiBFdcbYEdFCoEOfaS35inz1"
    );
    let hash = sha512crypt(b"x", b"$6$toolongsaltstring").unwrap();
    assert_eq!(
        hash,
        b"$6$toolongsaltstrin$m5UCdGkMg16fnZB/afayhHcFYEQvnTRoKod8GIJKB0rGGl9IQGUPKeGHHw5xPOFVPjhTAcuzioT6ZcDqsMKJ80"
    );
}

#[test]
fn donor_self_test() {
    let key = b"Xy01@#\x01\x02\x80\x7f\xff\r\n\x81\t !";
    let hash = sha512crypt(key, b"$6$rounds=1234$abc0123456789$").unwrap();
    assert_eq!(
        hash,
        b"$6$rounds=1234$abc0123456789$BCpt8zLrc/RcyuXmCDOE1ALqMXB2MH6n1g891HhFj8.w7LxGv.FTkqq6Vxc/km3Y0jE0j24jY5PIv/oOu6reg1"
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
    let hash = sha512crypt(b"pw", b"$6$rounds=1000$salt").unwrap();
    assert!(
        core::str::from_utf8(&hash)
            .unwrap()
            .starts_with("$6$rounds=1000$salt$")
    );
}

#[test]
fn salt_is_cut_at_sixteen() {
    let hash = sha512crypt(b"x", b"$6$toolongsaltstring").unwrap();
    assert!(
        core::str::from_utf8(&hash)
            .unwrap()
            .starts_with("$6$toolongsaltstrin$")
    );
}

#[test]
fn empty_salt_after_the_magic_is_mirrored() {
    let hash = sha512crypt(b"pw", b"$6$").unwrap();
    assert!(core::str::from_utf8(&hash).unwrap().starts_with("$6$$"));
}

#[test]
fn malformed_settings_are_rejected() {
    assert!(sha512crypt(b"pw", b"$5$salt").is_none());
    assert!(sha512crypt(b"pw", b"$6$rounds=1x$salt").is_none());
    assert!(sha512crypt(b"pw", b"$6$sa:lt").is_none());
    assert!(sha512crypt(b"pw", b"$5$sa\nlt").is_none());
    let long = alloc::vec![b'a'; 257];
    assert!(sha512crypt(&long, b"$6$salt").is_none());
}
