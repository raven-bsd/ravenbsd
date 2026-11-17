// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn accepts_no_arguments() {
    assert_eq!(parse_args([]), Ok(()));
}

#[test]
fn rejects_operands_and_options() {
    assert_eq!(parse_args(["x"]), Err(ParseError::UnexpectedOperand));
    assert_eq!(parse_args(["--help"]), Err(ParseError::UnexpectedOperand));
}

#[test]
fn formats_zero_and_fractions() {
    assert_eq!(format_one(0, 100), "0m0.00s");
    assert_eq!(format_one(150, 100), "0m1.50s");
}

#[test]
fn formats_minutes() {
    assert_eq!(format_one(9000, 100), "1m30.00s");
    assert_eq!(format_one(6000, 100), "1m0.00s");
}

#[test]
fn renders_two_posix_lines() {
    assert_eq!(
        render(0, 0, 0, 0, 100),
        "0m0.00s 0m0.00s\n0m0.00s 0m0.00s\n"
    );
}
