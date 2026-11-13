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

//! The shared scanner: prefixes, bases, saturation and the C23 binary rule.

use super::*;
use crate::IntStatus;

#[test]
fn decimal_plain() {
    let s = scan_integer(b"123456789", 10, true);
    assert_eq!(s.value, 123_456_789);
    assert!(!s.negative);
    assert_eq!(s.consumed, 9);
    assert_eq!(s.status, IntStatus::Ok);
}

#[test]
fn every_base_from_netbsd_t_strtol() {
    let vectors: &[(&[u8], i32, u64)] = &[
        (b"111010110111100110100010101", 2, 123_456_789),
        (b"22121022020212200", 3, 123_456_789),
        (b"13112330310111", 4, 123_456_789),
        (b"223101104124", 5, 123_456_789),
        (b"20130035113", 6, 123_456_789),
        (b"3026236221", 7, 123_456_789),
        (b"726746425", 8, 123_456_789),
        (b"277266780", 9, 123_456_789),
        (b"63762A05", 11, 123_456_789),
        (b"35418A99", 12, 123_456_789),
        (b"1C767471", 13, 123_456_789),
        (b"12579781", 14, 123_456_789),
        (b"AC89BC9", 15, 123_456_789),
        (b"75BCD15", 16, 123_456_789),
        (b"z", 36, 35),
    ];
    for (text, base, value) in vectors {
        let s = scan_integer(text, *base, true);
        assert_eq!(s.value, *value, "text {:?} base {base}", text);
        assert_eq!(s.status, IntStatus::Ok);
        assert_eq!(s.consumed, text.len());
    }
}

#[test]
fn base_zero_detection() {
    assert_eq!(scan_integer(b"01234567", 0, true).value, 342_391);
    assert_eq!(scan_integer(b"0123456789", 0, true).value, 342_391);
    assert_eq!(scan_integer(b"0123456789", 10, true).value, 123_456_789);
    assert_eq!(scan_integer(b"0x75bcd15", 0, true).value, 123_456_789);
}

#[test]
fn hex_prefix_needs_a_hex_digit() {
    let s = scan_integer(b" 0xX", 0, true);
    assert_eq!(s.value, 0);
    assert_eq!(s.consumed, 2);
    assert_eq!(s.status, IntStatus::Ok);
    let s = scan_integer(b" 0xX", 16, true);
    assert_eq!(s.value, 0);
    assert_eq!(s.consumed, 2);
}

#[test]
fn binary_prefix_needs_a_binary_digit() {
    assert_eq!(scan_integer(b"0b1011", 0, true).value, 11);
    assert_eq!(scan_integer(b"0B1011", 2, true).value, 11);
    let s = scan_integer(b"0b2", 0, true);
    assert_eq!(s.value, 0);
    assert_eq!(s.consumed, 1);
    let s = scan_integer(b"0b", 0, true);
    assert_eq!(s.value, 0);
    assert_eq!(s.consumed, 1);
}

#[test]
fn whitespace_is_exactly_the_c_set() {
    assert_eq!(scan_integer(b" \t\n\x0b\x0c\r42", 10, true).value, 42);
    // A space after the sign parses nothing.
    assert_eq!(scan_integer(b" - 5", 10, true).status, IntStatus::NoDigits);
    assert_eq!(
        scan_integer(b"\x1c42", 10, true).status,
        IntStatus::NoDigits
    );
}

#[test]
fn invalid_base() {
    for base in [-2i32, -1, 1, 37, 100] {
        let s = scan_integer(b"10", base, true);
        assert_eq!(s.status, IntStatus::InvalidBase);
        assert_eq!(s.consumed, 0);
        assert_eq!(s.value, 0);
    }
}

#[test]
fn no_digits_consumes_nothing() {
    for text in [b"".as_slice(), b"+", b"-", b"xyz", b"0x"] {
        let s = scan_integer(text, 10, false);
        if text == b"0x".as_slice() {
            assert_eq!(s.value, 0);
            assert_eq!(s.consumed, 1);
            continue;
        }
        assert_eq!(s.status, IntStatus::NoDigits, "{text:?}");
        assert_eq!(s.consumed, 0);
    }
}

#[test]
fn signed_saturation() {
    let s = scan_integer(b"9223372036854775808", 10, true);
    assert_eq!(s.status, IntStatus::Range);
    assert_eq!(s.value, i64::MAX as u64);
    assert_eq!(s.consumed, 19);
    let s = scan_integer(b"-9223372036854775809", 10, true);
    assert_eq!(s.status, IntStatus::Range);
    assert_eq!(s.value, i64::MIN as u64);
    assert_eq!(s.consumed, 20);
}

#[test]
fn unsigned_saturation() {
    let s = scan_integer(b"18446744073709551616", 10, false);
    assert_eq!(s.status, IntStatus::Range);
    assert_eq!(s.value, u64::MAX);
    assert_eq!(s.consumed, 20);
}

#[test]
fn wide_scan_uses_the_same_core() {
    let units: &[u32] = &[0x31, 0x32, 0x33];
    let s = scan_integer(units, 10, true);
    assert_eq!(s.value, 123);
    assert_eq!(s.consumed, 3);
    let units: &[u32] = &[0x31, 0x1000];
    let s = scan_integer(units, 10, true);
    assert_eq!(s.value, 1);
    assert_eq!(s.consumed, 1);
}

#[test]
fn whitespace_only_is_no_digits() {
    assert_eq!(scan_integer(b" \t\n", 10, true).status, IntStatus::NoDigits);
}

#[test]
fn sign_only_is_no_digits() {
    assert_eq!(scan_integer(b"-", 10, false).status, IntStatus::NoDigits);
}

#[test]
fn zero_prefix_and_binary() {
    assert_eq!(scan_integer(b"0", 0, true).value, 0);
    assert_eq!(scan_integer(b"0x0", 0, true).consumed, 3);
}

#[test]
fn hex_prefix_without_digits_parses_zero() {
    let scan = scan_integer(b"0xG", 0, true);
    assert_eq!(scan.value, 0);
    assert_eq!(scan.consumed, 1);
}

#[test]
fn binary_prefix_without_digits_parses_zero() {
    let scan = scan_integer(b"0b2", 2, true);
    assert_eq!(scan.value, 0);
    assert_eq!(scan.consumed, 1);
}

#[test]
fn base_36_upper_and_lower() {
    assert_eq!(scan_integer(b"Z", 36, true).value, 35);
    assert_eq!(scan_integer(b"z", 36, true).value, 35);
}

#[test]
fn negative_magnitude_keeps_negative_flag() {
    let scan = scan_integer(b"-42", 10, true);
    assert!(scan.negative);
    assert_eq!(scan.value, 42);
}

#[test]
fn unsigned_negative_limit_is_u64_max() {
    let scan = scan_integer(b"-18446744073709551616", 10, false);
    assert_eq!(scan.status, IntStatus::Range);
    assert_eq!(scan.value, u64::MAX);
}

#[test]
fn wide_unicode_digit_is_not_a_digit() {
    let units: &[u32] = &[0x0661];
    let scan = scan_integer(units, 10, true);
    assert_eq!(scan.status, IntStatus::NoDigits);
}

#[test]
fn wide_whitespace_is_ascii_only() {
    let units: &[u32] = &[0x00a0, 0x31];
    assert_eq!(scan_integer(units, 10, true).status, IntStatus::NoDigits);
}
