// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! bcrypt tests. The `$2a$` rows are cross-checked with the unmodified
//! `netbsd:lib/libcrypt/bcrypt.c` and with `musl:src/crypt/crypt_blowfish.c` (both produced the
//! same hash for the `abc` row); the `$2b$` rows come from the same donor with the ticket's
//! `$2b$` key rule (72 bytes, no NUL) applied to the minor switch and the key length:
//!
//! `cc -O2 -include shim.h -I. -o gen3 gen3.c bcrypt_b.c && ./gen3`
//!
//! (`bcrypt_b.c` is the donor copy with `case 'b':` added and `minor == 'a'` for the NUL
//! length; never committed.)

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

const ROWS: &[(&[u8], &[u8], &[u8])] = &[
    (
        b"password",
        b"$2b$04$abcdefghijklmnopqrstuu",
        b"$2b$04$abcdefghijklmnopqrstuuHq1QFV79p.2gtAgWqpJyLiGrJ/Z2Fza",
    ),
    (
        b"password",
        b"$2a$04$abcdefghijklmnopqrstuu",
        b"$2a$04$abcdefghijklmnopqrstuughE8Ev8uGFaUgY2cNEySvxngrb/Jzdm",
    ),
    (
        b"",
        b"$2b$04$abcdefghijklmnopqrstuu",
        b"$2b$04$abcdefghijklmnopqrstuubyCG3zY1GIXMyxfivm.ClDiInHzxjiq",
    ),
    (
        b"Hello world!",
        b"$2b$05$abcdefghijklmnopqrstuu",
        b"$2b$05$abcdefghijklmnopqrstuupcmfSSZo0h6yp/Yjt4uU63.1nrUFg.C",
    ),
    (
        b"abc",
        b"$2b$04$......................",
        b"$2b$04$......................IJ49QG.7p1jNXZ6WeHkH9.E.dYDSsDS",
    ),
    (
        b"abc",
        b"$2a$04$......................",
        b"$2a$04$......................ini1L2hXWkegMV822DC6vks..mWmZHK",
    ),
    (
        b"a",
        b"$2b$04$zzzzzzzzzzzzzzzzzzzzzz",
        b"$2b$04$zzzzzzzzzzzzzzzzzzzzzuzHdgGNwBj/e1D8auzP2AYtSPC4uqmQK",
    ),
    (
        b"tab\there",
        b"$2b$04$abcdefghijklmnopqrstuu",
        b"$2b$04$abcdefghijklmnopqrstuuMuE6xI7DTEBMDYkZUzpngBo8.lL1eUm",
    ),
    (
        b"secret",
        b"$2b$12$abcdefghijklmnopqrstuu",
        b"$2b$12$abcdefghijklmnopqrstuujSkoXmwKwKQniSEzkFftjx7Z1oDwCGy",
    ),
    (
        b"xy",
        b"$2b$04$ABCDEFGHIJKLMNOPQRSTUV",
        b"$2b$04$ABCDEFGHIJKLMNOPQRSTUOGeRpPqi7..7q/CFVGCkIz/yHNRt3vWW",
    ),
];

#[test]
fn donor_vectors() {
    for (password, setting, expected) in ROWS {
        assert_eq!(
            bcrypt(password, setting).as_deref(),
            Some(*expected),
            "password {password:?} setting {setting:?}"
        );
    }
}

#[test]
fn long_password_is_truncated_to_72_bytes() {
    // The oracle row's password: 90 decimal digits.
    let long: alloc::vec::Vec<u8> =
        b"012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789"
            .to_vec();
    let expected = b"$2b$04$abcdefghijklmnopqrstuum2G75IXDN/xsgbNa/hCiPSKyIHQd70S";
    assert_eq!(
        bcrypt(&long, b"$2b$04$abcdefghijklmnopqrstuu").as_deref(),
        Some(&expected[..])
    );
    // A 72-byte password gives the same hash as a longer one for 'b' (no NUL in the key).
    let exact = long.get(..72).unwrap_or(&long).to_vec();
    assert_eq!(
        bcrypt(&exact, b"$2b$04$abcdefghijklmnopqrstuu").as_deref(),
        Some(&expected[..])
    );
}

#[test]
fn rejected_settings() {
    for setting in [
        b"$2y$04$abcdefghijklmnopqrstuu".as_slice(),
        b"$2x$04$abcdefghijklmnopqrstuu",
        b"$3$04$abcdefghijklmnopqrstuu",
        b"$2b$03$abcdefghijklmnopqrstuu",
        b"$2b$32$abcdefghijklmnopqrstuu",
        b"$2b$04$short",
        b"$2b$4$abcdefghijklmnopqrstuu",
        b"$2b$0a$abcdefghijklmnopqrstuu",
        b"$1$salt",
        b"",
        b"$2b$04$abc$def",
    ] {
        assert!(bcrypt(b"pw", setting).is_none(), "setting {setting:?}");
    }
}

#[test]
fn output_is_at_most_60_bytes() {
    let hash = bcrypt(b"pw", b"$2b$04$abcdefghijklmnopqrstuu").unwrap();
    assert_eq!(hash.len(), 60);
}

#[test]
fn gensalt_is_deterministic_and_well_formed() {
    let mut first = FakeEntropy::new(7);
    let mut second = FakeEntropy::new(7);
    let mut a = [0u8; 30];
    let mut b = [0u8; 30];
    let na = bcrypt_gensalt(&mut first, 10, &mut a).unwrap();
    let nb = bcrypt_gensalt(&mut second, 10, &mut b).unwrap();
    assert_eq!(na, nb);
    assert_eq!(a, b);
    assert_eq!(na, 29);
    assert_eq!(&a[..7], b"$2a$10$");
    assert_eq!(a[29], 0);
}

#[test]
fn gensalt_clamps_log_rounds() {
    let mut entropy = FakeEntropy::new(1);
    let mut out = [0u8; 30];
    assert_eq!(bcrypt_gensalt(&mut entropy, 0, &mut out).unwrap(), 29);
    assert_eq!(&out[..7], b"$2a$04$");
    assert_eq!(bcrypt_gensalt(&mut entropy, 40, &mut out).unwrap(), 29);
    assert_eq!(&out[..7], b"$2a$31$");
}

#[test]
fn gensalt_output_hashes() {
    let mut entropy = FakeEntropy::new(7);
    let mut setting = [0u8; 30];
    let length = bcrypt_gensalt(&mut entropy, 4, &mut setting).unwrap();
    let hash = bcrypt(b"pw", &setting[..length]).unwrap();
    assert_eq!(&hash[..7], b"$2a$04$");
    assert_eq!(&hash[7..29], &setting[7..29]);
}
