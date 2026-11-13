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

//! `strtoul` vectors, including the T-5 negative rules.

use super::*;
use crate::conv::{strtoull, strtoumax};

#[test]
fn basic_vectors() {
    assert_eq!(strtoul(b"42", 10), (42, 2, IntStatus::Ok));
    assert_eq!(
        strtoul(b"0xffffffffffffffff", 16),
        (u64::MAX, 18, IntStatus::Ok)
    );
    assert_eq!(
        strtoul(b"18446744073709551615", 10),
        (u64::MAX, 20, IntStatus::Ok)
    );
}

#[test]
fn negative_input_wraps() {
    assert_eq!(strtoul(b"-1", 10), (u64::MAX, 2, IntStatus::Ok));
    assert_eq!(strtoul(b"-2", 10), (u64::MAX - 1, 2, IntStatus::Ok));
    assert_eq!(strtoul(b"-0", 10), (0, 2, IntStatus::Ok));
    assert_eq!(strtoul(b"-", 10), (0, 0, IntStatus::NoDigits));
}

#[test]
fn negative_overflow_does_not_negate() {
    assert_eq!(
        strtoul(b"-18446744073709551616", 10),
        (u64::MAX, 21, IntStatus::Range)
    );
    assert_eq!(
        strtoul(b"18446744073709551616", 10),
        (u64::MAX, 20, IntStatus::Range)
    );
}

#[test]
fn aliases() {
    assert_eq!(strtoull(b"77", 10), (77, 2, IntStatus::Ok));
    assert_eq!(strtoumax(b"-1", 10), (u64::MAX, 2, IntStatus::Ok));
}

#[test]
fn prefixes() {
    assert_eq!(strtoul(b"0b101", 0), (5, 5, IntStatus::Ok));
    assert_eq!(strtoul(b"0b101", 2), (5, 5, IntStatus::Ok));
    assert_eq!(strtoul(b"012", 0), (10, 3, IntStatus::Ok));
    assert_eq!(strtoul(b"012", 10), (12, 3, IntStatus::Ok));
}

#[test]
fn invalid_base() {
    assert_eq!(strtoul(b"1", 0 - 1), (0, 0, IntStatus::InvalidBase));
    assert_eq!(strtoul(b"1", 1), (0, 0, IntStatus::InvalidBase));
    assert_eq!(strtoul(b"1", 37), (0, 0, IntStatus::InvalidBase));
}

#[test]
fn unsigned_accepts_the_full_range() {
    assert_eq!(strtoul(b"0xffffffffffffffff", 0).0, u64::MAX);
}

#[test]
fn unsigned_zero_forms() {
    assert_eq!(strtoul(b"0", 0), (0, 1, IntStatus::Ok));
    assert_eq!(strtoul(b"00", 0), (0, 2, IntStatus::Ok));
    assert_eq!(strtoul(b"-0x0", 0), (0, 4, IntStatus::Ok));
}

#[test]
fn unsigned_prefix_requires_a_digit() {
    assert_eq!(strtoul(b"0x", 16), (0, 1, IntStatus::Ok));
    assert_eq!(strtoul(b"0b", 0), (0, 1, IntStatus::Ok));
}

#[test]
fn unsigned_no_digits() {
    assert_eq!(strtoul(b"   ", 10), (0, 0, IntStatus::NoDigits));
    assert_eq!(strtoul(b"xyz", 10), (0, 0, IntStatus::NoDigits));
}

#[test]
fn unsigned_invalid_bases() {
    for base in [1, 37, -2] {
        assert_eq!(strtoul(b"0", base), (0, 0, IntStatus::InvalidBase));
    }
}

#[test]
fn unsigned_saturation_keeps_all_digits_consumed() {
    assert_eq!(strtoul(b"999999999999999999999", 10).1, 21);
}

#[test]
fn unsigned_trailing_junk() {
    assert_eq!(strtoul(b"42junk", 10), (42, 2, IntStatus::Ok));
}

#[test]
fn unsigned_base_36_digits() {
    assert_eq!(strtoul(b"z", 36).0, 35);
    assert_eq!(strtoul(b"10", 36).0, 36);
}
