// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `hostname` pure logic.

use super::*;

#[test]
fn empty_args_prints() {
    assert_eq!(parse_args([].into_iter()), Ok(Action::Print));
}

#[test]
fn one_name_sets() {
    assert_eq!(parse_args(["raven"].into_iter()), Ok(Action::Set("raven")));
}

#[test]
fn selftest_is_recognised() {
    assert_eq!(parse_args(["--selftest"].into_iter()), Ok(Action::Selftest));
}

#[test]
fn extra_or_option_arguments_are_rejected() {
    assert_eq!(parse_args(["a", "b"].into_iter()), Err(UsageError));
    assert_eq!(parse_args(["-x"].into_iter()), Err(UsageError));
    assert_eq!(parse_args([""].into_iter()), Err(UsageError));
}

#[test]
fn names_are_bounded_and_printable() {
    assert!(valid_name("raven"));
    assert!(valid_name(&"a".repeat(HOSTNAME_MAX)));
    assert!(!valid_name(&"a".repeat(HOSTNAME_MAX + 1)));
    assert!(!valid_name("a/b"));
    assert!(!valid_name("a b\tc"));
    assert!(!valid_name(""));
}

#[test]
fn newline_trimming_is_single_and_tolerant() {
    assert_eq!(trim_newline(b"raven\n"), b"raven");
    assert_eq!(trim_newline(b"raven\r\n"), b"raven");
    assert_eq!(trim_newline(b"raven"), b"raven");
    assert_eq!(trim_newline(b"\n"), b"");
    assert_eq!(trim_newline(b""), b"");
}
