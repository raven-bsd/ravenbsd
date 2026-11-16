// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `diff`.

use super::{diff, parse};

#[test]
fn parse_unified() {
    let options = parse(["-u", "a", "b"]).unwrap();
    assert!(options.unified);
    assert_eq!(options.operands, vec!["a", "b"]);
    assert!(parse(["-x"]).is_err());
}

#[test]
fn equal_texts_are_empty() {
    let (changed, text) = diff("a\nb\n", "a\nb\n", false);
    assert!(!changed);
    assert!(text.is_empty());
}

#[test]
fn normal_change_and_delete() {
    let (changed, text) = diff("a\nb\n", "a\nc\n", false);
    assert!(changed);
    assert!(text.contains("2c2"));
    assert!(text.contains("< b"));
    assert!(text.contains("> c"));

    let (_, text) = diff("a\nb\n", "a\n", false);
    assert!(text.contains("2d1"), "{text}");
}

#[test]
fn normal_insertion() {
    let (_, text) = diff("a\n", "a\nb\n", false);
    assert!(text.contains("1a2"), "{text}");
    assert!(text.contains("> b"), "{text}");
}

#[test]
fn unified_headers_and_markers() {
    let (changed, text) = diff("a\nb\n", "a\nc\n", true);
    assert!(changed);
    assert!(
        text.starts_with("--- a\n+++ b\n@@ -1,2 +1,2 @@\n"),
        "{text}"
    );
    assert!(text.contains("-b\n+c\n"), "{text}");
}
