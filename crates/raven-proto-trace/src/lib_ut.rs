// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the trace-context wire codec.

use crate::{FIELD_LEN, TraceError, UNTRACED, encode, format, is_untraced, parse, parse_exact};

#[test]
fn format_is_little_endian() {
    assert_eq!(
        format(0x0102_0304_0506_0708),
        [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]
    );
}

#[test]
fn parse_round_trips() {
    for id in [0u64, 1, u64::MAX, 0xDEAD_BEEF_CAFE_F00D] {
        let bytes = format(id);
        assert_eq!(parse(&bytes), Ok(id));
        assert_eq!(parse_exact(&bytes), Ok(id));
    }
}

#[test]
fn parse_ignores_trailing_bytes_but_exact_rejects_them() {
    let mut bytes = [0u8; 12];
    bytes[..8].copy_from_slice(&format(0x1122_3344_5566_7788));
    // `parse` reads only the field; `parse_exact` is strict.
    assert_eq!(parse(&bytes), Ok(0x1122_3344_5566_7788));
    assert_eq!(parse_exact(&bytes), Err(TraceError::TrailingBytes));
}

#[test]
fn short_input_is_too_short_not_a_fault() {
    assert_eq!(parse(&[]), Err(TraceError::TooShort));
    assert_eq!(parse(&[0; 7]), Err(TraceError::TooShort));
    assert_eq!(parse_exact(&[0; 4]), Err(TraceError::TooShort));
    // A malformed field is a typed error, never a panic.
}

#[test]
fn encode_round_trips_and_rejects_short_output() {
    let mut out = [0u8; FIELD_LEN];
    assert_eq!(encode(&mut out, 42), Ok(()));
    assert_eq!(parse(&out), Ok(42));

    let mut short = [0u8; 4];
    assert_eq!(encode(&mut short, 42), Err(TraceError::TooShort));
}

#[test]
fn untraced_is_zero_only() {
    assert!(is_untraced(UNTRACED));
    assert!(is_untraced(0));
    assert!(!is_untraced(1));
    assert!(!is_untraced(u64::MAX));
}

#[test]
fn zero_field_is_missing_not_malformed() {
    // A zero field decodes successfully and is reported as "not traced" (start a new trace),
    // distinct from a malformed (too-short) field.
    assert_eq!(parse(&format(0)), Ok(0));
    assert!(is_untraced(parse(&format(0)).unwrap()));
}
