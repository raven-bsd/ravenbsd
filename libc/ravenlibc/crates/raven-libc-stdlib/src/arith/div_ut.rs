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

//! `div` vectors from NetBSD `t_div.c` and the `MIN / -1` rule.

use super::div;

#[test]
fn truncation() {
    assert_eq!(div(7, 2).quot, 3);
    assert_eq!(div(7, 2).rem, 1);
    assert_eq!(div(-7, 2).quot, -3);
    assert_eq!(div(-7, 2).rem, -1);
    assert_eq!(div(7, -2).quot, -3);
    assert_eq!(div(7, -2).rem, 1);
    assert_eq!(div(-7, -2).quot, 3);
    assert_eq!(div(-7, -2).rem, -1);
}

#[test]
fn min_by_minus_one() {
    let r = div(i32::MIN, -1);
    assert_eq!(r.quot, i32::MIN);
    assert_eq!(r.rem, 0);
}

#[test]
fn identity() {
    for n in [-100i32, -7, 0, 7, 100] {
        for d in [-13i32, -1, 1, 13] {
            let r = div(n, d);
            assert_eq!(r.quot * d + r.rem, n);
        }
    }
}
