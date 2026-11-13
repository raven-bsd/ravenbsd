// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn one(text: &str) -> Line {
    scan(text).remove(0)
}

#[test]
fn line_comment_is_blanked() {
    assert_eq!(one("let x = 1; // unsafe").code.trim(), "let x = 1;");
}

#[test]
fn string_contents_are_blanked_but_code_str_keeps_them() {
    let line = one("let x = \"unsafe\";");
    assert!(!line.code.contains("unsafe"));
    assert!(line.code_str.contains("unsafe"));
}

#[test]
fn raw_string_with_hashes() {
    let line = one("let x = r#\"unsafe { }\"#;");
    assert!(!line.code.contains("unsafe"));
}

#[test]
fn nested_block_comment() {
    let lines = scan("/* unsafe /* nested */ unsafe */ let x = 1;\n");
    assert!(!lines[0].code.contains("unsafe"));
}

#[test]
fn lifetime_is_not_a_char_literal() {
    let line = one("fn f<'a>(x: &'a u8) {}");
    assert!(line.code.contains("'a"));
}

#[test]
fn char_literal_is_blanked() {
    let line = one("let c = 'x';");
    assert!(!line.code.contains("'x'"));
}

#[test]
fn escaped_quote_in_string() {
    let line = one("let s = \"a\\\"unsafe\";");
    assert!(!line.code.contains("unsafe"));
}

#[test]
fn string_containing_slashes() {
    let line = one("let s = \"// not a comment\";");
    assert!(!line.code.contains("//"));
}

#[test]
fn comment_after_block_comment_is_collected() {
    let line = one("/* a */ let x = 1; // SAFETY: ok");
    assert!(line.comment.contains("SAFETY:"));
}

#[test]
fn unterminated_string_at_eof_does_not_panic() {
    let lines = scan("let s = \"unterminated");
    assert_eq!(lines.len(), 1);
}

#[test]
fn raw_string_across_lines() {
    let lines = scan("let s = r#\"a\nb\"#;\n");
    assert_eq!(lines.len(), 2);
    assert!(!lines[0].code.contains("unsafe"));
}

#[test]
fn byte_string_is_blanked() {
    let line = one("let b = b\"unsafe\";");
    assert!(!line.code.contains("unsafe"));
}

#[test]
fn code_keeps_columns() {
    let line = one("a // b");
    assert_eq!(line.code.len(), line.code_str.len());
}
