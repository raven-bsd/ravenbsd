// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uuid_create` tests: version and variant bits, determinism and the nil value.

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

#[test]
fn create_sets_version_4_and_variant_10() {
    let mut entropy = FakeEntropy::new(7);
    for _ in 0..64 {
        let uuid = create(&mut entropy);
        assert_eq!(uuid.time_hi_and_version >> 12, 4);

        assert_eq!(uuid.clock_seq_hi_and_reserved & 0xc0, 0x80);
    }
}

#[test]
fn create_is_deterministic_for_a_seed() {
    let mut first = FakeEntropy::new(7);
    let mut second = FakeEntropy::new(7);
    assert_eq!(create(&mut first), create(&mut second));
}

#[test]
fn two_draws_differ() {
    let mut entropy = FakeEntropy::new(7);
    assert_ne!(create(&mut entropy), create(&mut entropy));
}

#[test]
fn nil_is_all_zero() {
    let uuid = create_nil();
    assert_eq!(uuid.time_low, 0);
    assert_eq!(uuid.node, [0; 6]);
}
