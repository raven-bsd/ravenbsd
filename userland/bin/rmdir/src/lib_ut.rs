// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `rmdir`.

use super::parse;

#[test]
fn no_operands_is_empty() {
    assert!(parse([]).unwrap().operands.is_empty());
}

#[test]
fn one_operand_is_kept() {
    assert_eq!(parse(["/tmp/x"]).unwrap().operands, vec!["/tmp/x"]);
}

#[test]
fn multiple_operands_keep_order() {
    assert_eq!(parse(["a", "b"]).unwrap().operands, vec!["a", "b"]);
}

#[test]
fn double_dash_ends_options() {
    assert_eq!(parse(["--", "-x"]).unwrap().operands, vec!["-x"]);
}

#[test]
fn an_option_is_rejected() {
    assert!(parse(["-p"]).is_err());
}

#[test]
fn a_long_option_is_rejected() {
    assert!(parse(["--help"]).is_err());
}

#[test]
fn a_lone_dash_is_an_operand() {
    assert_eq!(parse(["-"]).unwrap().operands, vec!["-"]);
}
