// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Blowfish tests: the table checksum (recomputed from the donor source by the extraction
//! script), the state lifecycle and the zeroing.

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

fn table_fnv(blowfish: &Blowfish) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for value in blowfish.table.iter() {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

#[test]
fn table_matches_the_donor_extraction() {
    let blowfish = Blowfish::new();
    assert_eq!(table_fnv(&blowfish), TABLE_FNV1A);
}

#[test]
fn first_last_and_p_values() {
    let blowfish = Blowfish::new();
    assert_eq!(blowfish.s(0), 0xd131_0ba6);
    assert_eq!(blowfish.s(255), 0x6e85_076a);
    assert_eq!(blowfish.s(256), 0x4b7a_70e9);
    assert_eq!(blowfish.s(1023), 0x3ac3_72e6);
    assert_eq!(blowfish.p(0), 0x243f_6a88);
    assert_eq!(blowfish.p(17), 0x8979_fb1b);
}

#[test]
fn encipher_changes_the_state_and_is_deterministic() {
    let blowfish = Blowfish::new();
    let first = blowfish.encipher(0, 0);
    let second = blowfish.encipher(0, 0);
    assert_eq!(first, second);
    assert_ne!(first, (0, 0));
}

#[test]
fn expand0state_is_deterministic() {
    let mut first = Blowfish::new();
    let mut second = Blowfish::new();
    first.expand0state(b"key");
    second.expand0state(b"key");
    assert_eq!(first.encipher(1, 2), second.encipher(1, 2));
}

#[test]
fn expandstate_takes_the_salt() {
    let mut a = Blowfish::new();
    let mut b = Blowfish::new();
    a.expandstate(b"salt", b"key");
    b.expandstate(b"salt", b"key");
    assert_eq!(a.encipher(1, 2), b.encipher(1, 2));
    let mut c = Blowfish::new();
    c.expandstate(b"SALT", b"key");
    assert_ne!(a.encipher(1, 2), c.encipher(1, 2));
}

#[test]
fn zero_clears_the_schedule_but_not_the_algorithm() {
    let mut blowfish = Blowfish::new();
    blowfish.zero();
    assert_eq!(blowfish.s(0), 0);
    assert_eq!(blowfish.p(17), 0);
}

#[test]
fn encrypt_processes_pairs() {
    let mut data = [1u32, 2, 3, 4, 5];
    let blowfish = Blowfish::new();
    blowfish.encrypt(&mut data);
    assert_ne!(data[0], 1);
    assert_eq!(data[4], 5);
}

#[test]
fn donor_encipher_vectors() {
    let blowfish = Blowfish::new();
    let (left, right) = blowfish.encipher(0, 0);
    assert_eq!((left, right), (0x706d_9fcc, 0x1792_d23a));
    let mut keyed = Blowfish::new();
    keyed.expand0state(b"abc");
    let (left, right) = keyed.encipher(0, 0);
    assert_eq!((left, right), (0x1b90_6008, 0xc121_0e1c));
}
