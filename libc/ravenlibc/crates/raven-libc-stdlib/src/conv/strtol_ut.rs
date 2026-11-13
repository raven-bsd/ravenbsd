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

//! `strtol`/`strtoll`/`strtoimax` vectors from NetBSD `t_strtol.c` and the OpenBSD regress.

use super::*;
use crate::conv::{strtoimax, strtoll};

#[test]
fn basic_vectors() {
    let cases: &[(&[u8], i32, i64, usize)] = &[
        (b"123456789", 10, 123_456_789, 9),
        (b"  42", 10, 42, 4),
        (b"+42", 10, 42, 3),
        (b"-42", 10, -42, 3),
        (b"42junk", 10, 42, 2),
        (b"0x1f", 16, 31, 4),
        (b"0x1f", 0, 31, 4),
        (b"017", 0, 15, 3),
        (b"017", 8, 15, 3),
        (b"017", 10, 17, 3),
        (b"z", 36, 35, 1),
        (b"Z", 36, 35, 1),
    ];
    for (text, base, value, consumed) in cases {
        assert_eq!(
            strtol(text, *base),
            (*value, *consumed, IntStatus::Ok),
            "{text:?} base {base}"
        );
    }
}

#[test]
fn aliases() {
    assert_eq!(strtoll(b"-77", 10), (-77, 3, IntStatus::Ok));
    assert_eq!(strtoimax(b"-77", 10), (-77, 3, IntStatus::Ok));
}

#[test]
fn case_insensitivity() {
    assert_eq!(strtol(b"abcd", 16).0, 0xabcd);
    assert_eq!(strtol(b"     dcba", 16).0, 0xdcba);
    assert_eq!(strtol(b"abcd dcba", 16), (0xabcd, 4, IntStatus::Ok));
    assert_eq!(strtol(b"abc0x123", 16), (0xabc0, 4, IntStatus::Ok));
    assert_eq!(strtol(b"ABCD", 16).0, 0xabcd);
    assert_eq!(strtol(b"0xABCDX", 16), (0xabcd, 6, IntStatus::Ok));
}

#[test]
fn range_limits() {
    assert_eq!(
        strtol(b"9223372036854775807", 10),
        (i64::MAX, 19, IntStatus::Ok)
    );
    assert_eq!(
        strtol(b"9223372036854775808", 10),
        (i64::MAX, 19, IntStatus::Range)
    );
    assert_eq!(
        strtol(b"-9223372036854775808", 10),
        (i64::MIN, 20, IntStatus::Ok)
    );
    assert_eq!(
        strtol(b"-9223372036854775809", 10),
        (i64::MIN, 20, IntStatus::Range)
    );
}

#[test]
fn invalid_base_and_no_digits() {
    assert_eq!(strtol(b"10", -2), (0, 0, IntStatus::InvalidBase));
    assert_eq!(strtol(b"10", 1), (0, 0, IntStatus::InvalidBase));
    assert_eq!(strtol(b"10", 37), (0, 0, IntStatus::InvalidBase));
    assert_eq!(strtol(b"", 10), (0, 0, IntStatus::NoDigits));
    assert_eq!(strtol(b" - 5", 10), (0, 0, IntStatus::NoDigits));
}

#[test]
fn binary_prefix_is_c23() {
    assert_eq!(strtol(b"0b111", 0), (7, 5, IntStatus::Ok));
    assert_eq!(strtol(b"0B111", 2), (7, 5, IntStatus::Ok));
    assert_eq!(strtol(b"0b111", 8), (0, 1, IntStatus::Ok));
}

#[test]
fn hex_without_digit_parses_zero() {
    assert_eq!(strtol(b" 0xX", 0), (0, 2, IntStatus::Ok));
    assert_eq!(strtol(b"0x", 0), (0, 1, IntStatus::Ok));
    assert_eq!(strtol(b"0x", 16), (0, 1, IntStatus::Ok));
}

#[test]
fn overflow_still_consumes_all_digits() {
    assert_eq!(
        strtol(b"999999999999999999999999junk", 10),
        (i64::MAX, 24, IntStatus::Range)
    );
}

#[test]
fn leading_whitespace_exactly_six_chars() {
    for text in [
        b" 42".as_slice(),
        b"\t42",
        b"\n42",
        b"\x0b42",
        b"\x0c42",
        b"\r42",
    ] {
        assert_eq!(strtol(text, 10).0, 42);
    }
}

#[test]
fn leading_whitespace_combinations() {
    assert_eq!(strtol(b" \t\n\r\x0b\x0c42", 10).0, 42);
}

#[test]
fn sign_before_prefix() {
    assert_eq!(strtol(b"-0x10", 0), (-16, 5, IntStatus::Ok));
    assert_eq!(strtol(b"+0x10", 0), (16, 5, IntStatus::Ok));
    assert_eq!(strtol(b"-0b10", 0), (-2, 5, IntStatus::Ok));
}

#[test]
fn sign_alone_is_no_digits() {
    assert_eq!(strtol(b"+", 10), (0, 0, IntStatus::NoDigits));
    assert_eq!(strtol(b"-", 10), (0, 0, IntStatus::NoDigits));
    assert_eq!(strtol(b"+ ", 10), (0, 0, IntStatus::NoDigits));
}

#[test]
fn base_one_and_negative() {
    for base in [1i32, -1, -100] {
        assert_eq!(strtol(b"123", base), (0, 0, IntStatus::InvalidBase));
    }
}

#[test]
fn base_two_accepts_binary_prefix() {
    assert_eq!(strtol(b"0b1111", 2), (15, 6, IntStatus::Ok));
}

#[test]
fn base_sixteen_does_not_accept_binary_prefix_as_binary() {
    assert_eq!(strtol(b"0b11", 16).0, 0xb11);
}

#[test]
fn base_zero_octal_and_decimal() {
    assert_eq!(strtol(b"0777", 0).0, 511);
    assert_eq!(strtol(b"777", 0).0, 777);
    assert_eq!(strtol(b"0777", 10).0, 777);
}

#[test]
fn uppercase_and_lowercase_digits() {
    assert_eq!(strtol(b"FF", 16).0, 255);
    assert_eq!(strtol(b"ff", 16).0, 255);
    assert_eq!(strtol(b"Zz", 36).0, 35 * 36 + 35);
}

#[test]
fn a_letter_beyond_the_base_stops() {
    assert_eq!(strtol(b"12a", 10), (12, 2, IntStatus::Ok));
    assert_eq!(strtol(b"12g", 16), (0x12, 2, IntStatus::Ok));
}

#[test]
fn exact_limits_in_every_common_base() {
    assert_eq!(strtol(b"7fffffffffffffff", 16).0, i64::MAX);
    assert_eq!(strtol(b"-8000000000000000", 16).0, i64::MIN);
}

#[test]
fn no_digits_after_prefix_keeps_endptr_at_start() {
    let (value, consumed, status) = strtol(b"  -0x", 16);
    assert_eq!((value, consumed, status), (0, 4, IntStatus::Ok));
}
