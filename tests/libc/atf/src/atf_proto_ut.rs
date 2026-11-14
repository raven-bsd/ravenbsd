// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

const OK: &str = "Content-Type: application/X-atf-tp; version=\"1\"\n\nident: one\ndescr: first\n\nident: two\ntimeout: 5\nrequire.user: root\n";

#[test]
fn parses_two_cases_and_preserves_metadata() {
    let cases = parse_list(OK).expect("parse");
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].ident, "one");
    assert_eq!(
        cases[0].meta.get("descr").map(String::as_str),
        Some("first")
    );
    assert_eq!(cases[1].ident, "two");
    assert_eq!(cases[1].meta.get("timeout").map(String::as_str), Some("5"));
    assert_eq!(
        cases[1].meta.get("require.user").map(String::as_str),
        Some("root")
    );
}

#[test]
fn accepts_the_empty_list() {
    let cases = parse_list("Content-Type: application/X-atf-tp; version=\"1\"\n\n").expect("parse");
    assert!(cases.is_empty());
}

#[test]
fn accepts_no_final_newline() {
    let text = "Content-Type: application/X-atf-tp; version=\"1\"\n\nident: one";
    assert_eq!(parse_list(text).expect("parse").len(), 1);
}

#[test]
fn rejects_a_wrong_content_type() {
    let error = parse_list("Content-Type: text/plain\n\nident: one\n").expect_err("must fail");
    assert_eq!(error.line, 1);
    assert!(error.message.contains("Content-Type"));
}

#[test]
fn rejects_a_missing_blank_line() {
    let error = parse_list("Content-Type: application/X-atf-tp; version=\"1\"\nident: one\n")
        .expect_err("must fail");
    assert_eq!(error.line, 2);
}

#[test]
fn rejects_a_duplicate_ident() {
    let text = "Content-Type: application/X-atf-tp; version=\"1\"\n\nident: one\n\nident: one\n";
    let error = parse_list(text).expect_err("must fail");
    assert!(error.message.contains("duplicate ident"));
}

#[test]
fn rejects_a_block_without_ident() {
    let text = "Content-Type: application/X-atf-tp; version=\"1\"\n\ndescr: no ident\n";
    let error = parse_list(text).expect_err("must fail");
    assert!(error.message.contains("ident"));
}

#[test]
fn rejects_ident_not_first() {
    let text = "Content-Type: application/X-atf-tp; version=\"1\"\n\ndescr: x\nident: one\n";
    assert!(parse_list(text).is_err());
}

#[test]
fn rejects_two_blank_lines_between_blocks() {
    let text = "Content-Type: application/X-atf-tp; version=\"1\"\n\nident: one\n\n\nident: two\n";
    let error = parse_list(text).expect_err("must fail");
    assert!(error.message.contains("consecutive"));
}

#[test]
fn rejects_a_line_without_a_value_separator() {
    let text = "Content-Type: application/X-atf-tp; version=\"1\"\n\nident one\n";
    let error = parse_list(text).expect_err("must fail");
    assert!(error.line == 3);
}

#[test]
fn rejects_a_duplicate_metadata_key() {
    let text =
        "Content-Type: application/X-atf-tp; version=\"1\"\n\nident: one\ndescr: a\ndescr: b\n";
    let error = parse_list(text).expect_err("must fail");
    assert!(error.message.contains("duplicate metadata key"));
}

#[test]
fn error_display_names_the_line() {
    let error = parse_list("bad\n").expect_err("must fail");
    assert!(error.to_string().starts_with("line 1:"));
}

#[test]
fn empty_input_is_an_error() {
    assert!(parse_list("").is_err());
}
