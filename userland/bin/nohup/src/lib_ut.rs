// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `nohup` parser.

use super::*;
use alloc::vec;

#[test]
fn parses_command_and_arguments() {
    let options = parse_args(["/bin/echo", "-n", "hi"]).unwrap();
    assert_eq!(options.command, vec!["/bin/echo", "-n", "hi"]);
}

#[test]
fn double_dash_allows_a_dash_command() {
    let options = parse_args(["--", "-x"]).unwrap();
    assert_eq!(options.command, vec!["-x"]);
}

#[test]
fn rejects_missing_command_and_options() {
    assert_eq!(
        parse_args(Vec::<&str>::new()),
        Err(UsageError::MissingCommand)
    );
    assert_eq!(parse_args(["-x"]), Err(UsageError::UnknownOption));
}
