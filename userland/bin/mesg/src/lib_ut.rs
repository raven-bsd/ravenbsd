// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `mesg` parser and mode bits.

use super::*;

#[test]
fn parses_query_and_both_answers() {
    assert_eq!(parse_args(Vec::<&str>::new()), Ok(Action::Query));
    assert_eq!(parse_args(["y"]), Ok(Action::Set(true)));
    assert_eq!(parse_args(["yes"]), Ok(Action::Set(true)));
    assert_eq!(parse_args(["n"]), Ok(Action::Set(false)));
    assert_eq!(parse_args(["no"]), Ok(Action::Set(false)));
}

#[test]
fn rejects_extra_or_unknown_arguments() {
    assert_eq!(parse_args(["y", "n"]), Err(UsageError));
    assert_eq!(parse_args(["maybe"]), Err(UsageError));
}

#[test]
fn mode_bit_is_set_and_cleared() {
    assert!(allows(0o620));
    assert!(!allows(0o600));
    assert_eq!(apply(0o600, true), 0o620);
    assert_eq!(apply(0o620, false), 0o600);
}
