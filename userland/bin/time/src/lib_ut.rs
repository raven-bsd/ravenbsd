// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn parses_portable_command() {
    let options = parse_args(["-p", "sleep", "1"]).expect("options");
    assert!(options.portable);
    assert_eq!(options.command, ["sleep", "1"]);
}

#[test]
fn rejects_missing_utility() {
    assert_eq!(parse_args(["-p"]), Err(ParseError::MissingUtility));
}

#[test]
fn rejects_unknown_option_before_utility() {
    assert_eq!(parse_args(["-x", "ls"]), Err(ParseError::UnknownOption));
}

#[test]
fn options_after_the_utility_belong_to_it() {
    let options = parse_args(["ls", "-l"]).expect("options");
    assert_eq!(options.command, ["ls", "-l"]);
}

#[test]
fn formats_three_lines() {
    assert_eq!(
        format(1.25, 0.0, 0.0, false),
        "real 1.25\nuser 0.00\nsys 0.00\n"
    );
}
