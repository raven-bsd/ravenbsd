// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! `rand_r` vectors from the donor formula.

use super::*;

#[test]
fn vector() {
    let mut seed = 1u32;
    let expected = [
        1_103_515_245u32.wrapping_add(12_345) & 0x7fff_ffff,
        (1_103_515_245u32.wrapping_add(12_345))
            .wrapping_mul(1_103_515_245)
            .wrapping_add(12_345)
            & 0x7fff_ffff,
    ];
    assert_eq!(rand_r(&mut seed), expected[0]);
    assert_eq!(rand_r(&mut seed), expected[1]);
}

#[test]
fn stays_in_range() {
    let mut seed = 0xdead_beefu32;
    for _ in 0..10_000 {
        assert!(rand_r(&mut seed) <= 0x7fff_ffff);
    }
}

#[test]
fn zero_seed_is_valid() {
    let mut seed = 0u32;
    assert_eq!(rand_r(&mut seed), 12_345 & 0x7fff_ffff);
    assert_eq!(seed, 12_345);
}
