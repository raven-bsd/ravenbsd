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

//! `atoi` vectors from NetBSD `t_atoi.c`.

use super::*;

#[test]
fn basic() {
    assert_eq!(atoi(b"0"), 0);
    assert_eq!(atoi(b"1"), 1);
    assert_eq!(atoi(b"-1"), -1);
    assert_eq!(atoi(b"+1"), 1);
    assert_eq!(atoi(b"  123"), 123);
    assert_eq!(atoi(b"123abc"), 123);
    assert_eq!(atoi(b"abc"), 0);
    assert_eq!(atoi(b""), 0);
}

#[test]
fn truncates_like_the_c_cast() {
    assert_eq!(atoi(b"2147483647"), i32::MAX);
    assert_eq!(atoi(b"2147483648"), i32::MIN);
    assert_eq!(atoi(b"-2147483648"), i32::MIN);
    assert_eq!(atoi(b"-2147483649"), i32::MAX);
}

#[test]
fn whitespace_and_sign() {
    assert_eq!(atoi(b" \t\n-42"), -42);
    assert_eq!(atoi(b"+42"), 42);
}

#[test]
fn prefix_is_decimal_only() {
    assert_eq!(atoi(b"0x10"), 0);
    assert_eq!(atoi(b"010"), 10);
}

#[test]
fn empty_and_junk() {
    assert_eq!(atoi(b""), 0);
    assert_eq!(atoi(b"x1"), 0);
    assert_eq!(atoi(b"-"), 0);
}

#[test]
fn overflow_truncates_twice() {
    assert_eq!(atoi(b"4294967296"), 0);
    assert_eq!(atoi(b"4294967297"), 1);
}

#[test]
fn plus_zero() {
    assert_eq!(atoi(b"+0"), 0);
}
