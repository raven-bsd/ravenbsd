// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uuid_from_string`/`uuid_to_string` tests: the `uuidtest.c` rows and the round trip.

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

const SAMPLE: &[u8] = b"f81d4fae-7dec-11d0-a765-00a0c91e6bf6";

#[test]
fn parse_and_print_roundtrip() {
    let uuid = from_string(SAMPLE).unwrap();
    let mut out = [0u8; 37];
    to_string(&uuid, &mut out);

    assert_eq!(&out[..36], SAMPLE);
    assert_eq!(out[36], 0);
}

#[test]
fn upper_case_is_accepted_and_lower_case_printed() {
    let uuid = from_string(b"F81D4FAE-7DEC-11D0-A765-00A0C91E6BF6").unwrap();
    let mut out = [0u8; 37];
    to_string(&uuid, &mut out);
    assert_eq!(&out[..36], SAMPLE);
}

#[test]
fn empty_string_is_nil() {
    assert_eq!(from_string(b"").unwrap(), crate::uuid::Uuid::default());
}

#[test]
fn field_values() {
    let uuid = from_string(SAMPLE).unwrap();
    assert_eq!(uuid.time_low, 0xf81d4fae);
    assert_eq!(uuid.time_mid, 0x7dec);
    assert_eq!(uuid.time_hi_and_version, 0x11d0);
    assert_eq!(uuid.clock_seq_hi_and_reserved, 0xa7);
    assert_eq!(uuid.clock_seq_low, 0x65);
    assert_eq!(uuid.node, [0x00, 0xa0, 0xc9, 0x1e, 0x6b, 0xf6]);
}

#[test]
fn wrong_length_is_invalid() {
    for text in [
        b"f81d4fae-7dec-11d0-a765-00a0c91e6bf".as_slice(),
        b"f81d4fae-7dec-11d0-a765-00a0c91e6bf67",
        b"",
    ] {
        if text.is_empty() {
            assert!(from_string(text).is_ok());
        } else {
            assert_eq!(from_string(text), Err(Status::InvalidString));
        }
    }
}

#[test]
fn bad_hyphens_are_invalid() {
    assert_eq!(
        from_string(b"f81d4fae_7dec-11d0-a765-00a0c91e6bf6"),
        Err(Status::InvalidString)
    );
    assert_eq!(
        from_string(b"f81d4fae-7dec_11d0-a765-00a0c91e6bf6"),
        Err(Status::InvalidString)
    );
}

#[test]
fn non_hex_is_invalid() {
    assert_eq!(
        from_string(b"f81d4faX-7dec-11d0-a765-00a0c91e6bf6"),
        Err(Status::InvalidString)
    );
}

#[test]
fn variant_zero_one_and_two_are_accepted() {
    // variant 0 (NCS): clock_seq_hi < 0x80
    assert!(from_string(b"f81d4fae-7dec-11d0-0765-00a0c91e6bf6").is_ok());
    // variant 1 (DCE): 10xx
    assert!(from_string(b"f81d4fae-7dec-11d0-a765-00a0c91e6bf6").is_ok());
    // variant 2 (Microsoft): 110x
    assert!(from_string(b"f81d4fae-7dec-11d0-c765-00a0c91e6bf6").is_ok());
    // variant 3: 111x -> bad version
    assert_eq!(
        from_string(b"f81d4fae-7dec-11d0-e765-00a0c91e6bf6"),
        Err(Status::BadVersion)
    );
}

#[test]
fn to_string_nul_terminates() {
    let uuid = from_string(SAMPLE).unwrap();
    let mut out = [b'X'; 37];
    to_string(&uuid, &mut out);
    assert_eq!(out.len(), 37);
    assert_eq!(out[36], 0);
}
