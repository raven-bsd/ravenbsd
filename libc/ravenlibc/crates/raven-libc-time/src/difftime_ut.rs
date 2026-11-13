// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `difftime` tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

#[test]
fn ordinary_differences() {
    assert_eq!(difftime(10, 3), 7.0);
    assert_eq!(difftime(3, 10), -7.0);
    assert_eq!(difftime(5, 5), 0.0);
}

#[test]
fn opposite_signs_do_not_wrap() {
    assert_eq!(difftime(1, -1), 2.0);
    assert_eq!(difftime(-1, 1), -2.0);
    assert_eq!(difftime(i64::MAX, i64::MIN), (u64::MAX) as f64);
    assert_eq!(difftime(i64::MIN, i64::MAX), -(u64::MAX as f64));
}

#[test]
fn values_near_two_to_the_53_keep_the_exact_difference() {
    let base = (1i64 << 53) - 4;
    assert_eq!(difftime(base + 3, base), 3.0);
    assert_eq!(difftime(base, base + 3), -3.0);
}
