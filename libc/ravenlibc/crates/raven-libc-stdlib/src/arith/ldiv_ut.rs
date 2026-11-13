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

//! `ldiv` and the `lldiv`/`imaxdiv` aliases.

use super::ldiv;

#[test]
fn truncation() {
    assert_eq!(ldiv(7, 2).quot, 3);
    assert_eq!(ldiv(-7, 2).rem, -1);
    assert_eq!(ldiv(7, -2).rem, 1);
    assert_eq!(ldiv(-7, -2).quot, 3);
}

#[test]
fn min_by_minus_one() {
    let r = ldiv(i64::MIN, -1);
    assert_eq!(r.quot, i64::MIN);
    assert_eq!(r.rem, 0);
}
