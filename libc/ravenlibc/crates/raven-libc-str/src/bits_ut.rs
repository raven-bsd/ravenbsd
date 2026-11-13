// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::{ffs, ffsl, ffsll, fls, flsl, flsll};

#[test]
fn ffs_zero_is_zero() {
    assert_eq!(ffs(0), 0);
    assert_eq!(ffsl(0), 0);
    assert_eq!(ffsll(0), 0);
}

#[test]
fn ffs_is_one_based() {
    assert_eq!(ffs(1), 1);
    assert_eq!(ffs(2), 2);
    assert_eq!(ffs(3), 1);
    assert_eq!(ffs(0x8000_0000u32 as i32), 32);
    assert_eq!(ffs(i32::MIN), 32);
    assert_eq!(ffs(i32::MAX), 1);
}

#[test]
fn ffsl_boundaries() {
    assert_eq!(ffsl(1), 1);
    assert_eq!(ffsl(i64::MIN), 64);
    assert_eq!(ffsl(i64::MAX), 1);
    assert_eq!(ffsll(i64::MIN), 64);
}

#[test]
fn every_i32_pattern() {
    for value in (0u32..=0xffff).chain((0..=0xffff).map(|v| v << 16)) {
        let expected = if value == 0 {
            0
        } else {
            value.trailing_zeros() as i32 + 1
        };
        assert_eq!(ffs(value as i32), expected, "{value:#x}");
    }
}

#[test]
fn fls_family() {
    assert_eq!(fls(0), 0);
    assert_eq!(fls(1), 1);
    assert_eq!(fls(2), 2);
    assert_eq!(fls(i32::MAX), 31);
    assert_eq!(fls(i32::MIN), 32);
    assert_eq!(flsl(i64::MIN), 64);
    assert_eq!(flsll(i64::MAX), 63);
    assert_eq!(flsll(0), 0);
}

#[test]
fn fls_matches_bit_index() {
    for value in (0u32..=0xffff).chain((0..=0xffff).map(|v| v << 16)) {
        let expected = if value == 0 {
            0
        } else {
            32 - value.leading_zeros() as i32
        };
        assert_eq!(fls(value as i32), expected, "{value:#x}");
    }
}
