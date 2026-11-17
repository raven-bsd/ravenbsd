// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `mesg` parser and mode bits.

use super::*;

#[test]
fn parses_query_and_both_answers() {
    assert_eq!(
        parse_args(Vec::<&str>::new()).unwrap().action,
        Action::Query
    );
    assert_eq!(parse_args(["y"]).unwrap().action, Action::Set(true));
    assert_eq!(parse_args(["yes"]).unwrap().action, Action::Set(true));
    assert_eq!(parse_args(["n"]).unwrap().action, Action::Set(false));
    assert_eq!(parse_args(["no"]).unwrap().action, Action::Set(false));
}

#[test]
fn parses_an_explicit_target() {
    let options = parse_args(["-t", "/tmp/notty", "n"]).unwrap();
    assert_eq!(options.target.as_deref(), Some("/tmp/notty"));
    assert_eq!(options.action, Action::Set(false));
    assert_eq!(parse_args(["--tty"]), Err(UsageError));
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
