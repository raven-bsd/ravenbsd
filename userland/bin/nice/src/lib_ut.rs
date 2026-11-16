// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `nice` parser.

use super::*;
use alloc::vec;

#[test]
fn no_arguments_is_a_query() {
    let options = parse_args(Vec::<&str>::new()).unwrap();
    assert_eq!(options.increment, None);
    assert!(options.command.is_empty());
}

#[test]
fn parses_increment_and_command() {
    let options = parse_args(["-n", "5", "/bin/true"]).unwrap();
    assert_eq!(options.increment, Some(5));
    assert_eq!(options.command, vec!["/bin/true"]);
}

#[test]
fn parses_shorthand_increment() {
    let options = parse_args(["-5", "/bin/true"]).unwrap();
    assert_eq!(options.increment, Some(-5));
    assert_eq!(options.command, vec!["/bin/true"]);
}

#[test]
fn command_arguments_are_not_reparsed_as_options() {
    let options = parse_args(["-n", "5", "/bin/echo", "-n", "hi"]).unwrap();
    assert_eq!(options.command, vec!["/bin/echo", "-n", "hi"]);
}

#[test]
fn double_dash_allows_a_dash_command() {
    let options = parse_args(["--", "-5"]).unwrap();
    assert_eq!(options.increment, None);
    assert_eq!(options.command, vec!["-5"]);
}

#[test]
fn rejects_bad_increments() {
    assert_eq!(parse_args(["-n"]), Err(UsageError::BadIncrement));
    assert_eq!(parse_args(["-n", "x"]), Err(UsageError::BadIncrement));
}
