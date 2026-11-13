// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the musl `fnmatch` port.
// Test-only code is exempt from the production lints (17-crate-layout 9.3).
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use super::*;
use crate::locale::Utf8Locale;

const U: Utf8Locale = Utf8Locale;

fn m(pat: &str, s: &str, flags: u32) -> i32 {
    fnmatch(pat.as_bytes(), s.as_bytes(), flags, &U)
}

#[test]
fn literals_and_nomatch() {
    assert_eq!(m("abc", "abc", 0), 0);
    assert_eq!(m("abc", "abd", 0), FNM_NOMATCH);
    assert_eq!(m("", "", 0), 0);
    assert_eq!(m("", "x", 0), FNM_NOMATCH);
    assert_eq!(m("x", "", 0), FNM_NOMATCH);
}

#[test]
fn star_and_question() {
    assert_eq!(m("*", "", 0), 0);
    assert_eq!(m("*", "anything", 0), 0);
    assert_eq!(m("a*c", "abc", 0), 0);
    assert_eq!(m("a*c", "ac", 0), 0);
    assert_eq!(m("a*c", "ab", 0), FNM_NOMATCH);
    assert_eq!(m("?", "a", 0), 0);
    assert_eq!(m("?", "", 0), FNM_NOMATCH);
    assert_eq!(m("a?c", "abc", 0), 0);
    assert_eq!(m("a?c", "ac", 0), FNM_NOMATCH);
    assert_eq!(m("a**c", "ac", 0), 0);
}

#[test]
fn brackets() {
    assert_eq!(m("[abc]", "a", 0), 0);
    assert_eq!(m("[abc]", "d", 0), FNM_NOMATCH);
    assert_eq!(m("[!abc]", "d", 0), 0);
    assert_eq!(m("[^abc]", "d", 0), 0);
    assert_eq!(m("[!abc]", "a", 0), FNM_NOMATCH);
    assert_eq!(m("[a-z]", "m", 0), 0);
    assert_eq!(m("[a-z]", "M", 0), FNM_NOMATCH);
    assert_eq!(m("[]a]", "]", 0), 0);
    assert_eq!(m("[]a]", "a", 0), 0);
    assert_eq!(m("[!]a]", "b", 0), 0);
    assert_eq!(m("[a-]", "a", 0), 0);
    assert_eq!(m("[a-]", "-", 0), 0);
    assert_eq!(m("[", "[", 0), 0);
    assert_eq!(m("[abc", "[abc", 0), 0);
}

#[test]
fn bracket_class_and_ranges() {
    assert_eq!(m("[[:alpha:]]", "a", 0), 0);
    assert_eq!(m("[[:alpha:]]", "1", 0), FNM_NOMATCH);
    assert_eq!(m("[[:digit:]]", "7", 0), 0);
    assert_eq!(m("[[:upper:]]", "Q", 0), 0);
    assert_eq!(m("[[:upper:]]", "q", 0), FNM_NOMATCH);
    assert_eq!(m("[[:alpha:][:digit:]]", "5", 0), 0);
    assert_eq!(m("[[:nonexistent:]]", "x", 0), FNM_NOMATCH);
    assert_eq!(m("[[:alpha:]]", "é", 0), 0);
    assert_eq!(m("[a-é]", "é", 0), 0);
}

#[test]
fn utf8_question_and_invalid_bytes() {
    assert_eq!(m("?", "é", 0), 0);
    assert_eq!(m("?", "€", 0), 0);
    assert_eq!(m("?", "😀", 0), 0);
    assert_eq!(m("??", "é", 0), FNM_NOMATCH);
    assert_eq!(m("a?c", "aé", 0), FNM_NOMATCH);
    let invalid = b"\xff";
    assert_eq!(fnmatch(b"?", invalid, 0, &U), FNM_NOMATCH);
    assert_eq!(fnmatch(b"*", invalid, 0, &U), 0);
    assert_eq!(fnmatch(b"a*", b"a\xffb", 0, &U), 0);
    assert_eq!(fnmatch(b"a?b", b"a\xffb", 0, &U), FNM_NOMATCH);
}

#[test]
fn period_with_and_without_pathname() {
    assert_eq!(m("*", ".x", 0), 0);
    assert_eq!(m("*", ".x", FNM_PERIOD), FNM_NOMATCH);
    assert_eq!(m(".*", ".x", FNM_PERIOD), 0);
    assert_eq!(m(".?x", ".ax", FNM_PERIOD), 0);
    assert_eq!(m("*", "a/.x", FNM_PERIOD), 0);
    assert_eq!(m("*", "a/.x", FNM_PERIOD | FNM_PATHNAME), FNM_NOMATCH);
    assert_eq!(m("*/*", "a/.x", FNM_PERIOD | FNM_PATHNAME), FNM_NOMATCH);
    assert_eq!(m("*/.*", "a/.x", FNM_PERIOD | FNM_PATHNAME), 0);
    assert_eq!(m("\\*", ".x", FNM_PERIOD), FNM_NOMATCH);
    assert_eq!(m("*", ".x", FNM_PERIOD | FNM_CASEFOLD), FNM_NOMATCH);
}

#[test]
fn pathname_and_leading_dir() {
    assert_eq!(m("a/b", "a/b", FNM_PATHNAME), 0);
    assert_eq!(m("a?b", "a/b", FNM_PATHNAME), FNM_NOMATCH);
    assert_eq!(m("a*b", "a/b", FNM_PATHNAME), FNM_NOMATCH);
    assert_eq!(m("*b", "/b", FNM_PATHNAME), FNM_NOMATCH);
    assert_eq!(m("*b", "/b", 0), 0);
    assert_eq!(m("test", "test/", FNM_LEADING_DIR), 0);
    assert_eq!(m("test", "testing", FNM_LEADING_DIR), FNM_NOMATCH);
    assert_eq!(m("test/this", "test/", 0), FNM_NOMATCH);
    assert_eq!(m("test*/this", "test/this", 0), 0);
}

#[test]
fn casefold() {
    assert_eq!(m("xxx", "XxX", 0), FNM_NOMATCH);
    assert_eq!(m("xxx", "XxX", FNM_CASEFOLD), 0);
    assert_eq!(m("é", "É", FNM_CASEFOLD), 0);
    assert_eq!(m("É", "é", FNM_CASEFOLD), 0);
    assert_eq!(m("[a-z]", "Q", FNM_CASEFOLD), 0);
    assert_eq!(m("[À-Þ]", "é", FNM_CASEFOLD), 0);
}

#[test]
fn noescape() {
    assert_eq!(m("\\*", "*", 0), 0);
    assert_eq!(m("\\*", "*", FNM_NOESCAPE), FNM_NOMATCH);
    assert_eq!(m("\\*", "\\x", FNM_NOESCAPE), 0);
    assert_eq!(m("\\", "\\", 0), 0);
    assert_eq!(m("\\", "\\", FNM_NOESCAPE), 0);
    assert_eq!(m("a\\", "a\\", 0), 0);
    assert_eq!(m("", "\\", 0), FNM_NOMATCH);
}

#[test]
fn pathological_star_runs_are_fast() {
    let subject = "a".repeat(40);
    let start = std::time::Instant::now();
    assert_eq!(m("*a*a*a*a*a*b", &subject, 0), FNM_NOMATCH);
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
}
