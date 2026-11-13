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

//! The LP64 aliases: `llabs`/`imaxabs` and `lldiv`/`imaxdiv`.

use super::{DivT, LDivT, imaxabs, imaxdiv, llabs, lldiv};

#[test]
fn llabs_and_imaxabs_alias_labs() {
    assert_eq!(llabs(0), 0);
    assert_eq!(llabs(-42), 42);
    assert_eq!(llabs(i64::MIN), i64::MIN);
    assert_eq!(imaxabs(-42), 42);
    assert_eq!(imaxabs(i64::MAX), i64::MAX);
}

#[test]
fn lldiv_and_imaxdiv_alias_ldiv() {
    assert_eq!(lldiv(7, 2), LDivT { quot: 3, rem: 1 });
    assert_eq!(imaxdiv(-7, 2), LDivT { quot: -3, rem: -1 });
    assert_eq!(lldiv(i64::MIN, -1).quot, i64::MIN);
}

#[test]
fn div_remainder_sign_follows_dividend() {
    let d: DivT = super::div(-5, 3);
    assert_eq!(d.rem, -2);
}

#[test]
fn llabs_zero() {
    assert_eq!(llabs(0), 0);
}

#[test]
fn imaxabs_min() {
    assert_eq!(imaxabs(i64::MIN), i64::MIN);
}

#[test]
fn lldiv_identity_negative() {
    let result = lldiv(-9, 4);
    assert_eq!(result.quot, -2);
    assert_eq!(result.rem, -1);
}

#[test]
fn imaxdiv_identity() {
    let result = imaxdiv(9, -4);
    assert_eq!(result.quot, -2);
    assert_eq!(result.rem, 1);
}

#[test]
fn div_by_one() {
    assert_eq!(super::div(7, 1), DivT { quot: 7, rem: 0 });
    assert_eq!(super::div(-7, 1), DivT { quot: -7, rem: 0 });
}
