// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the command-line parser.

use super::*;

#[test]
fn an_empty_line_is_empty() {
    assert_eq!(parse_args([]), Ok(Options::default()));
    assert!(Options::default().is_empty());
}

#[test]
fn flags_are_collected() {
    let options = parse_args(["-l", "-s", "-u", "-c"]).expect("options");
    assert!(options.list);
    assert!(options.stats);
    assert!(options.usdt);
    assert!(options.clear);
    assert!(!options.is_empty());
}

#[test]
fn values_are_parsed() {
    let options = parse_args(["-e", "1", "-d", "2", "-r", "10"]).expect("options");
    assert_eq!(options.enable, Some(1));
    assert_eq!(options.disable, Some(2));
    assert_eq!(options.read, Some(10));
}

#[test]
fn help_is_a_flag() {
    assert!(parse_args(["-h"]).expect("options").help);
    assert!(parse_args(["--help"]).expect("options").help);
}

#[test]
fn unknown_missing_and_invalid_values_are_rejected() {
    assert_eq!(parse_args(["-x"]), Err(ArgError::Unknown));
    assert_eq!(parse_args(["-e"]), Err(ArgError::MissingValue));
    assert_eq!(parse_args(["-d"]), Err(ArgError::MissingValue));
    assert_eq!(parse_args(["-r"]), Err(ArgError::MissingValue));
    assert_eq!(parse_args(["-e", "x"]), Err(ArgError::InvalidValue));
    assert_eq!(parse_args(["-r", "x"]), Err(ArgError::InvalidValue));
}
