// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `cat` argument handling.

use super::parse;

#[test]
fn no_operands_is_an_empty_list() {
    assert!(parse([]).unwrap().operands.is_empty());
}

#[test]
fn one_file_is_one_operand() {
    assert_eq!(parse(["a"]).unwrap().operands, vec!["a"]);
}

#[test]
fn several_files_keep_order() {
    assert_eq!(
        parse(["a", "b", "c"]).unwrap().operands,
        vec!["a", "b", "c"]
    );
}

#[test]
fn double_dash_ends_options() {
    assert_eq!(parse(["--", "-n"]).unwrap().operands, vec!["-n"]);
}

#[test]
fn a_lone_dash_is_a_file_name() {
    assert_eq!(parse(["-"]).unwrap().operands, vec!["-"]);
}

#[test]
fn an_option_is_rejected() {
    assert!(parse(["-n"]).is_err());
}

#[test]
fn a_double_dash_option_is_rejected() {
    assert!(parse(["--help"]).is_err());
}

#[test]
fn operands_after_double_dash_are_kept() {
    assert_eq!(parse(["a", "--", "b"]).unwrap().operands, vec!["a", "b"]);
}
