// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `date` pure logic.

use super::*;
use std::string::String;

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
    assert_eq!(parse_args(["--selftest", "x"].into_iter()), Err(UsageError));
}

#[test]
fn uptime_format_is_stable() {
    let mut text = String::new();
    format_uptime(0, &mut text).unwrap();
    assert_eq!(text, "up 0s");
    text.clear();
    format_uptime(42, &mut text).unwrap();
    assert_eq!(text, "up 42s");
    text.clear();
    format_uptime(3600, &mut text).unwrap();
    assert_eq!(text, "up 3600s");
}
