// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `dmesg` pure logic.

use super::*;

#[test]
fn empty_args_prints() {
    assert_eq!(parse_args([].into_iter()), Ok(Action::Print));
}

#[test]
fn selftest_is_recognised() {
    assert_eq!(parse_args(["--selftest"].into_iter()), Ok(Action::Selftest));
}

#[test]
fn double_dash_is_accepted_as_no_operand() {
    assert_eq!(parse_args(["--"].into_iter()), Ok(Action::Print));
}

#[test]
fn extra_or_unknown_arguments_are_rejected() {
    assert_eq!(parse_args(["x"].into_iter()), Err(UsageError));
    assert_eq!(parse_args(["--", "x"].into_iter()), Err(UsageError));
    assert_eq!(parse_args(["--selftest", "x"].into_iter()), Err(UsageError));
}

#[test]
fn record_count_ignores_a_trailing_fragment() {
    assert_eq!(record_count(b""), 0);
    assert_eq!(record_count(b"one\n"), 1);
    assert_eq!(record_count(b"one\ntwo\n"), 2);
    assert_eq!(record_count(b"one\ntwo"), 1);
    assert_eq!(record_count(b"\n"), 1);
}
