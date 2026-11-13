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

//! `strtonum` bounds and texts from OpenBSD `strtonumtest.c`.

use super::*;
#[test]
fn accept_in_range() {
    assert_eq!(strtonum(b"0", -10, 10), Ok(0));
    assert_eq!(strtonum(b"10", 0, 10), Ok(10));
    assert_eq!(strtonum(b"-10", -10, 0), Ok(-10));
    assert_eq!(strtonum(b"  +7", -10, 10), Ok(7));
    assert_eq!(strtonum(b"-9223372036854775808", i64::MIN, 0), Ok(i64::MIN));
    assert_eq!(strtonum(b"9223372036854775807", 0, i64::MAX), Ok(i64::MAX));
}

#[test]
fn invalid_inputs() {
    assert_eq!(strtonum(b"", -1, 1), Err(StrtonumError::Invalid));
    assert_eq!(strtonum(b"x", -1, 1), Err(StrtonumError::Invalid));
    assert_eq!(strtonum(b"1x", -1, 1), Err(StrtonumError::Invalid));
    assert_eq!(strtonum(b"1 ", -1, 1), Err(StrtonumError::Invalid));
    assert_eq!(strtonum(b"1", 2, 1), Err(StrtonumError::Invalid));
    assert_eq!(strtonum(b"-", -1, 1), Err(StrtonumError::Invalid));
}

#[test]
fn bounds() {
    assert_eq!(strtonum(b"-11", -10, 10), Err(StrtonumError::TooSmall));
    assert_eq!(strtonum(b"11", -10, 10), Err(StrtonumError::TooLarge));
    assert_eq!(
        strtonum(b"-9223372036854775809", i64::MIN, i64::MAX),
        Err(StrtonumError::TooSmall)
    );
    assert_eq!(
        strtonum(b"9223372036854775808", i64::MIN, i64::MAX),
        Err(StrtonumError::TooLarge)
    );
}

#[test]
fn messages_are_the_donor_texts() {
    assert_eq!(StrtonumError::Invalid.as_bytes(), b"invalid");
    assert_eq!(StrtonumError::TooSmall.as_bytes(), b"too small");
    assert_eq!(StrtonumError::TooLarge.as_bytes(), b"too large");
    assert_eq!(StrtonumError::Invalid.as_c_bytes(), b"invalid\0");
    assert_eq!(StrtonumError::TooSmall.as_c_bytes(), b"too small\0");
    assert_eq!(StrtonumError::TooLarge.as_c_bytes(), b"too large\0");
}

#[test]
fn leading_plus_and_spaces() {
    assert_eq!(strtonum(b"   +42", 0, 100), Ok(42));
    assert_eq!(strtonum(b"\t42", 0, 100), Ok(42));
}

#[test]
fn trailing_space_is_invalid() {
    assert_eq!(strtonum(b"42 ", 0, 100), Err(StrtonumError::Invalid));
    assert_eq!(strtonum(b"42\t", 0, 100), Err(StrtonumError::Invalid));
}

#[test]
fn zero_bounds_only_accept_zero() {
    assert_eq!(strtonum(b"0", 0, 0), Ok(0));
    assert_eq!(strtonum(b"1", 0, 0), Err(StrtonumError::TooLarge));
    assert_eq!(strtonum(b"-1", 0, 0), Err(StrtonumError::TooSmall));
}

#[test]
fn inverted_bounds_are_invalid_before_parsing() {
    assert_eq!(strtonum(b"junk", 10, 0), Err(StrtonumError::Invalid));
    assert_eq!(strtonum(b"", 10, 0), Err(StrtonumError::Invalid));
}

#[test]
fn negative_zero_is_ok() {
    assert_eq!(strtonum(b"-0", -1, 1), Ok(0));
}

#[test]
fn huge_negative_is_too_small() {
    assert_eq!(
        strtonum(b"-999999999999999999999", i64::MIN, i64::MAX),
        Err(StrtonumError::TooSmall)
    );
}

#[test]
fn huge_positive_is_too_large() {
    assert_eq!(
        strtonum(b"999999999999999999999", i64::MIN, i64::MAX),
        Err(StrtonumError::TooLarge)
    );
}

#[test]
fn hex_text_is_not_decimal() {
    assert_eq!(strtonum(b"0x10", 0, 100), Err(StrtonumError::Invalid));
}
