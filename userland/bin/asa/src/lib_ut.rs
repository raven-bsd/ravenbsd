// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `asa` carriage-control model.

use super::{UsageError, interpret, parse};

#[test]
fn empty_input_produces_no_output() {
    assert!(interpret(&[]).is_empty());
}

#[test]
fn a_space_is_single_spacing_and_the_control_byte_is_removed() {
    assert_eq!(interpret(b" text\n"), b"text\n");
}

#[test]
fn the_first_line_never_starts_with_a_blank_line() {
    assert_eq!(interpret(b" first\n second\n"), b"first\nsecond\n");
}

#[test]
fn zero_double_spaces_the_first_line() {
    assert_eq!(interpret(b"0double\n"), b"\ndouble\n");
}

#[test]
fn zero_double_spaces_later_lines() {
    assert_eq!(interpret(b" one\n0two\n"), b"one\n\ntwo\n");
}

#[test]
fn one_breaks_the_page() {
    assert_eq!(interpret(b"1page\n"), b"\x0cpage\n");
    assert_eq!(interpret(b" one\n1two\n"), b"one\x0ctwo\n");
}

#[test]
fn plus_overstrikes_the_previous_line() {
    assert_eq!(interpret(b"+first\n"), b"first\n");
    assert_eq!(interpret(b" one\n+two\n"), b"one\rtwo\n");
}

#[test]
fn an_unknown_control_is_single_spacing() {
    assert_eq!(interpret(b" one\n9two\n"), b"one\ntwo\n");
}

#[test]
fn an_empty_line_is_a_blank_line() {
    assert_eq!(interpret(b" one\n\n two\n"), b"one\n\ntwo\n");
}

#[test]
fn a_missing_final_newline_still_ends_the_output() {
    assert_eq!(interpret(b" last"), b"last\n");
}

#[test]
fn non_utf8_content_is_preserved() {
    assert_eq!(interpret(b" ab\xff\n"), b"ab\xff\n");
}

#[test]
fn all_controls_combine_in_order() {
    assert_eq!(
        interpret(b" page1\n0double\n1next\n+over\n?unknown\n"),
        b"page1\n\ndouble\x0cnext\rover\nunknown\n"
    );
}

#[test]
fn a_control_only_line_keeps_the_advance() {
    assert_eq!(interpret(b" one\n0\n"), b"one\n\n\n");
}

#[test]
fn no_operands_means_standard_input() {
    assert!(parse([]).unwrap().is_empty());
}

#[test]
fn file_operands_keep_order() {
    assert_eq!(parse(["a", "b"]).unwrap(), vec!["a", "b"]);
}

#[test]
fn a_lone_dash_is_the_standard_input_operand() {
    assert_eq!(parse(["-"]).unwrap(), vec!["-"]);
}

#[test]
fn double_dash_ends_option_processing() {
    assert_eq!(parse(["--", "-x"]).unwrap(), vec!["-x"]);
}

#[test]
fn an_option_is_rejected() {
    assert_eq!(parse(["-x"]).unwrap_err(), UsageError::UnknownOption);
    assert_eq!(parse(["--help"]).unwrap_err(), UsageError::UnknownOption);
}
