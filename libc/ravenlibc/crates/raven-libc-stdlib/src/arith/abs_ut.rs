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

//! `abs` boundary tests from NetBSD `t_abs.c` and the C UB case of A-stdlib-12.

use super::abs;

#[test]
fn basic() {
    assert_eq!(abs(0), 0);
    assert_eq!(abs(1), 1);
    assert_eq!(abs(-1), 1);
    assert_eq!(abs(123456), 123_456);
}

#[test]
fn boundaries() {
    assert_eq!(abs(i32::MAX), i32::MAX);
    assert_eq!(abs(i32::MIN), i32::MIN);
    assert_eq!(abs(i32::MIN + 1), i32::MAX);
}
