// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests of the utility constants.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

#[test]
fn nc_is_256_in_byte_mode_and_128_in_utf8() {
    assert_eq!(nc(1), 256);
    assert_eq!(nc(4), 128);
    assert_eq!(nc(2), 128);
}

#[test]
fn nc_max_is_always_256() {
    assert_eq!(NC_MAX, 256);
    assert_eq!(CHAR_MIN, -128);
    assert_eq!(CHAR_MAX, 127);
}

#[test]
fn uch_truncates_to_a_byte() {
    assert_eq!(uch(0x41), 0x41);
    assert_eq!(uch(0x1e9), 0xe9);
    assert_eq!(uch(-1), 0xff);
}

#[test]
fn out_and_ign_are_below_every_character() {
    const {
        assert!(OUT < 0);
        assert!(IGN < OUT);
    }
    assert_eq!(OUT, CHAR_MIN - 1);
    assert_eq!(IGN, CHAR_MIN - 2);
}

#[test]
fn dupmax_and_infinity_are_reexported() {
    assert_eq!(DUPMAX, 255);
    assert_eq!(INFINITY, 256);
}
