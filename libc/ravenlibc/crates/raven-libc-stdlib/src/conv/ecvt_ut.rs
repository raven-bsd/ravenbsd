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

//!
//! Host tests for the `ecvt_r`/`fcvt_r` layouts. The expected strings and decimal points are
//! the values a C library returns for the same `double` (checked against `fcvt`/`ecvt` on the
//! host; the layout is pure, so the digits are passed in directly).

use raven_libc_abi::errno::{EINVAL, Errno};

use crate::conv::ecvt::{ecvt_digits, fcvt_digits};

fn text(buffer: &[u8], converted: crate::conv::ecvt::Converted) -> &[u8] {
    buffer.get(..converted.len).unwrap_or(&[])
}

#[test]
fn ecvt_pads_to_the_requested_digits() {
    let mut out = [0u8; 8];
    let result = ecvt_digits(4, false, b"15", 1, None, &mut out).expect("ecvt");
    assert_eq!(result.len, 4);
    assert_eq!(result.decpt, 1);
    assert_eq!(text(&out, result), b"1500");
    assert_eq!(out.get(4), Some(&0));
}

#[test]
fn ecvt_keeps_a_negative_exponent() {
    let mut out = [0u8; 8];
    let result = ecvt_digits(4, false, b"1", -2, None, &mut out).expect("ecvt");
    assert_eq!(result.decpt, -2);
    assert_eq!(text(&out, result), b"1000");
}

#[test]
fn ecvt_zero_starts_at_one() {
    let mut out = [0u8; 8];
    let result = ecvt_digits(4, false, b"0", 1, None, &mut out).expect("ecvt");
    assert_eq!(result.decpt, 1);
    assert_eq!(text(&out, result), b"0000");
}

#[test]
fn ecvt_special_text_is_not_padded() {
    let mut out = [0u8; 8];
    let result = ecvt_digits(4, true, b"", 0, Some(b"inf"), &mut out).expect("ecvt");
    assert_eq!(result.decpt, 0);
    assert!(result.negative);
    assert_eq!(text(&out, result), b"inf");

    let result = ecvt_digits(2, false, b"", 0, Some(b"nan"), &mut out).expect("ecvt");
    assert_eq!(text(&out, result), b"na");
}

#[test]
fn ecvt_rejects_a_short_buffer() {
    let mut out = [0u8; 4];
    assert!(ecvt_digits(4, false, b"15", 1, None, &mut out).is_err());
    let mut empty = [];
    assert!(ecvt_digits(0, false, b"0", 1, None, &mut empty).is_err());
}

#[test]
fn fcvt_pads_to_the_point_plus_places() {
    let mut out = [0u8; 8];
    let result = fcvt_digits(2, false, b"15", 1, None, &mut out).expect("fcvt");
    assert_eq!(result.decpt, 1);
    assert_eq!(text(&out, result), b"150");

    let result = fcvt_digits(2, false, b"12346", 3, None, &mut out).expect("fcvt");
    assert_eq!(result.decpt, 3);
    assert_eq!(text(&out, result), b"12346");
}

#[test]
fn fcvt_zero_pads_below_the_point() {
    let mut out = [0u8; 8];
    let result = fcvt_digits(2, false, b"0", 1, None, &mut out).expect("fcvt");
    assert_eq!(result.decpt, 1);
    assert_eq!(text(&out, result), b"000");

    let result = fcvt_digits(2, true, b"0", 1, None, &mut out).expect("fcvt");
    assert!(result.negative);
    assert_eq!(text(&out, result), b"000");
}

#[test]
fn fcvt_rounds_below_the_precision_to_empty() {
    let mut out = [0u8; 8];
    let result = fcvt_digits(2, false, b"0", -2, None, &mut out).expect("fcvt");
    assert_eq!(result.len, 0);
    assert_eq!(result.decpt, -2);
    assert_eq!(out.first(), Some(&0));
}

#[test]
fn fcvt_extends_a_large_exponent() {
    let mut out = [0u8; 32];
    let result = fcvt_digits(2, false, b"1", 21, None, &mut out).expect("fcvt");
    assert_eq!(result.len, 23);
    assert_eq!(result.decpt, 21);
    assert_eq!(out.first(), Some(&b'1'));
    assert!(
        text(&out, result)
            .get(1..)
            .is_some_and(|rest| rest.iter().all(|byte| *byte == b'0'))
    );
}

#[test]
fn fcvt_special_text() {
    let mut out = [0u8; 8];
    let result = fcvt_digits(2, false, b"", 0, Some(b"nan"), &mut out).expect("fcvt");
    assert_eq!(result.decpt, 0);
    assert_eq!(text(&out, result), b"nan");
}

#[test]
fn fcvt_rejects_a_short_buffer() {
    let mut out = [0u8; 2];
    assert!(fcvt_digits(2, false, b"0", 1, None, &mut out).is_err());
    let mut empty = [];
    assert!(fcvt_digits(0, false, b"0", -1, None, &mut empty).is_err());
}

#[test]
fn invalid_errno_is_einval() {
    let mut out = [0u8; 1];
    let error = fcvt_digits(2, false, b"0", 1, None, &mut out).expect_err("short");
    assert_eq!(error, Errno(EINVAL as u16));
}
