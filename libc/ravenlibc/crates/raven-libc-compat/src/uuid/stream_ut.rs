// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uuid_enc_le`/`uuid_dec_le`/`uuid_enc_be`/`uuid_dec_be` tests.

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
use crate::uuid::Uuid;

fn sample() -> Uuid {
    Uuid {
        time_low: 0x0011_2233,
        time_mid: 0x4455,
        time_hi_and_version: 0x6677,
        clock_seq_hi_and_reserved: 0x88,

        clock_seq_low: 0x99,
        node: [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff],
    }
}

#[test]
fn little_endian_layout() {
    let mut buf = [0u8; 16];
    enc_le(&sample(), &mut buf);
    assert_eq!(
        buf,
        [
            0x33, 0x22, 0x11, 0x00, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff
        ]
    );
    assert_eq!(dec_le(&buf), sample());
}

#[test]
fn big_endian_layout() {
    let mut buf = [0u8; 16];
    enc_be(&sample(), &mut buf);
    assert_eq!(
        buf,
        [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff
        ]
    );
    assert_eq!(dec_be(&buf), sample());
}

#[test]
fn roundtrip_for_both_orders() {
    let uuid = crate::uuid::from_string(b"f81d4fae-7dec-11d0-a765-00a0c91e6bf6").unwrap();
    let mut buf = [0u8; 16];
    enc_le(&uuid, &mut buf);
    assert_eq!(dec_le(&buf), uuid);
    enc_be(&uuid, &mut buf);
    assert_eq!(dec_be(&buf), uuid);
}
