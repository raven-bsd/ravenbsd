// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `sleep` pure logic.

use super::*;

#[test]
fn parses_zero_and_positive_values() {
    assert_eq!(parse_seconds("0"), Ok(0));
    assert_eq!(parse_seconds("1"), Ok(1));
    assert_eq!(parse_seconds("42"), Ok(42));
}

#[test]
fn rejects_empty_malformed_and_negative_values() {
    assert_eq!(parse_seconds(""), Err(UsageError));
    assert_eq!(parse_seconds("x"), Err(UsageError));
    assert_eq!(parse_seconds("-1"), Err(UsageError));
    assert_eq!(parse_seconds("1.5"), Err(UsageError));
    assert_eq!(parse_seconds(" 1"), Err(UsageError));
}

#[test]
fn rejects_values_beyond_the_nanosecond_range() {
    assert_eq!(parse_seconds("18446744074"), Err(UsageError));
    assert_eq!(parse_seconds("999999999999999999999"), Err(UsageError));
}

#[test]
fn converts_seconds_to_nanoseconds() {
    assert_eq!(to_nanos(0), Some(0));
    assert_eq!(to_nanos(1), Some(1_000_000_000));
    assert_eq!(to_nanos(3), Some(3_000_000_000));
}

#[test]
fn conversion_overflow_is_none() {
    assert_eq!(to_nanos(MAX_SECONDS).is_some(), true);
    assert_eq!(to_nanos(MAX_SECONDS + 1), None);
}
