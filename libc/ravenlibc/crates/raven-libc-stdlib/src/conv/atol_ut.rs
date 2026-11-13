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

//! `atol`/`atoll` vectors.

use super::*;

#[test]
fn basic() {
    assert_eq!(atol(b"0"), 0);
    assert_eq!(atol(b"-42"), -42);
    assert_eq!(atol(b"42x"), 42);
    assert_eq!(atol(b"x"), 0);
}

#[test]
fn long_long_is_the_same_body() {
    assert_eq!(atoll(b"-9223372036854775808"), i64::MIN);
    assert_eq!(atoll(b"9223372036854775808"), i64::MAX);
    for value in [-1000i64, -1, 0, 1, 1000] {
        let text = match value {
            -1000 => b"-1000".as_slice(),
            -1 => b"-1".as_slice(),
            0 => b"0".as_slice(),
            1 => b"1".as_slice(),
            _ => b"1000".as_slice(),
        };
        assert_eq!(atoll(text), value);
    }
}

#[test]
fn whitespace_and_junk() {
    assert_eq!(atol(b"  -12junk"), -12);
    assert_eq!(atol(b"junk"), 0);
}

#[test]
fn decimal_only() {
    assert_eq!(atol(b"010"), 10);
    assert_eq!(atol(b"0x10"), 0);
}

#[test]
fn atoll_limit() {
    assert_eq!(atoll(b"9223372036854775807"), i64::MAX);
    assert_eq!(atoll(b"-9223372036854775808"), i64::MIN);
}

#[test]
fn atoll_overflow_saturates() {
    assert_eq!(atoll(b"9223372036854775808"), i64::MAX);
    assert_eq!(atoll(b"-9223372036854775809"), i64::MIN);
}
