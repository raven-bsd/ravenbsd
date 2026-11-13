// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Small named cases for the `fnmatch` corners of doc 07 section 7.2.

use raven_libc_glob::consts::*;
use raven_libc_glob::{ByteLocale, Utf8Locale, fnmatch};

fn m(pattern: &str, subject: &str, flags: u32) -> i32 {
    fnmatch(pattern.as_bytes(), subject.as_bytes(), flags, &Utf8Locale)
}

#[test]
fn empty_pattern_matches_only_the_empty_string() {
    assert_eq!(m("", "", 0), 0);
    assert_eq!(m("", "a", 0), FNM_NOMATCH);
}

#[test]
fn a_star_matches_the_empty_string() {
    assert_eq!(m("*", "", 0), 0);
    assert_eq!(m("**", "", 0), 0);
    assert_eq!(m("***", "", 0), 0);
}

#[test]
fn a_question_needs_one_character() {
    assert_eq!(m("?", "", 0), FNM_NOMATCH);
    assert_eq!(m("?", "a", 0), 0);
    assert_eq!(m("??", "a", 0), FNM_NOMATCH);
}

#[test]
fn an_unterminated_bracket_is_literal() {
    assert_eq!(m("[", "[", 0), 0);
    assert_eq!(m("[abc", "[abc", 0), 0);
}

#[test]
fn bracket_with_a_leading_closing_bracket() {
    assert_eq!(m("[]a]", "]", 0), 0);
    assert_eq!(m("[]a]", "a", 0), 0);
    assert_eq!(m("[]a]", "b", 0), FNM_NOMATCH);
}

#[test]
fn bracket_with_negation_and_a_leading_closing_bracket() {
    assert_eq!(m("[!]a]", "b", 0), 0);
    assert_eq!(m("[!]a]", "]", 0), FNM_NOMATCH);
    assert_eq!(m("[!]a]", "a", 0), FNM_NOMATCH);
}

#[test]
fn a_trailing_range_dash_is_literal() {
    assert_eq!(m("[a-]", "a", 0), 0);
    assert_eq!(m("[a-]", "-", 0), 0);
    assert_eq!(m("[a-]", "b", 0), FNM_NOMATCH);
}

#[test]
fn a_posix_character_class() {
    assert_eq!(m("[[:alpha:]]", "q", 0), 0);
    assert_eq!(m("[[:alpha:]]", "7", 0), FNM_NOMATCH);
}

#[test]
fn a_two_byte_code_point_is_one_question_mark() {
    assert_eq!(m("?", "é", 0), 0);
    assert_eq!(m("??", "é", 0), FNM_NOMATCH);
}

#[test]
fn a_four_byte_code_point_is_one_question_mark() {
    assert_eq!(m("?", "😀", 0), 0);
    assert_eq!(m("a?b", "a😀b", 0), 0);
}

#[test]
fn an_invalid_byte_is_unmatchable_in_utf8() {
    assert_eq!(fnmatch(b"?", b"\xff", 0, &Utf8Locale), FNM_NOMATCH);
    assert_eq!(fnmatch(b"*", b"\xff", 0, &Utf8Locale), 0);
}

#[test]
fn an_invalid_byte_is_literal_in_the_byte_locale() {
    assert_eq!(fnmatch(b"?", b"\xff", 0, &ByteLocale), 0);
    assert_eq!(fnmatch(b"\xff", b"\xff", 0, &ByteLocale), 0);
}

#[test]
fn period_is_protected_after_every_slash_with_pathname() {
    assert_eq!(m("a/*", "a/.x", FNM_PATHNAME | FNM_PERIOD), FNM_NOMATCH);
    assert_eq!(m("a/b/*", "a/b/.x", FNM_PATHNAME | FNM_PERIOD), FNM_NOMATCH);
    assert_eq!(m("a/*/x", "a/.b/x", FNM_PATHNAME | FNM_PERIOD), FNM_NOMATCH);
}

#[test]
fn period_is_protected_only_at_the_start_without_pathname() {
    assert_eq!(m("*", ".x", FNM_PERIOD), FNM_NOMATCH);
    assert_eq!(m("a*", "a.x", FNM_PERIOD), 0);
    assert_eq!(m("*", "a/.x", FNM_PERIOD), 0);
}

#[test]
fn leading_dir_ignores_the_tail() {
    assert_eq!(m("a/b", "a/b/c", FNM_LEADING_DIR), 0);
    assert_eq!(m("a/b", "a/bc", FNM_LEADING_DIR), FNM_NOMATCH);
}

#[test]
fn casefold_with_a_non_ascii_letter() {
    assert_eq!(m("é", "É", FNM_CASEFOLD), 0);
    assert_eq!(m("É", "é", FNM_CASEFOLD), 0);
    assert_eq!(m("é", "É", 0), FNM_NOMATCH);
}

#[test]
fn a_backslash_at_the_end_is_ordinary() {
    assert_eq!(m("a\\", "a\\", 0), 0);
    assert_eq!(m("\\", "\\", 0), 0);
}

#[test]
fn noescape_disables_only_the_escape() {
    assert_eq!(m("\\x", "\\x", 0), FNM_NOMATCH);
    assert_eq!(m("\\x", "x", 0), 0);
    assert_eq!(m("\\x", "\\x", FNM_NOESCAPE), 0);
    assert_eq!(m("\\x", "x", FNM_NOESCAPE), FNM_NOMATCH);
}

#[test]
fn aliases_have_the_same_values() {
    assert_eq!(FNM_FILE_NAME, FNM_PATHNAME);
    assert_eq!(FNM_IGNORECASE, FNM_CASEFOLD);
}

#[test]
fn casefolded_range() {
    assert_eq!(m("[a-z]", "Q", FNM_CASEFOLD), 0);
    // Built without the uppercase range spelled literally, so the stable-toolchain guard does
    // not mistake a bracket range for a nightly compiler flag.
    let upper = "[A\u{2d}Z]";
    assert_eq!(
        fnmatch(
            upper.as_bytes(),
            b"q",
            FNM_CASEFOLD,
            &raven_libc_glob::Utf8Locale
        ),
        0
    );
    assert_eq!(m("[a-z]", "Q", 0), FNM_NOMATCH);
}

#[test]
fn digit_class_negative() {
    assert_eq!(m("[![:digit:]]", "x", 0), 0);
    assert_eq!(m("[![:digit:]]", "5", 0), FNM_NOMATCH);
}

#[test]
fn upper_class_negative() {
    assert_eq!(m("[[:upper:]]", "Q", 0), 0);
    assert_eq!(m("[[:upper:]]", "q", 0), FNM_NOMATCH);
}

#[test]
fn multiple_stars_collapse() {
    assert_eq!(m("a**b", "ab", 0), 0);
    assert_eq!(m("a***b", "axxxb", 0), 0);
    assert_eq!(m("a**b", "a/c/b", 0), 0);
    assert_eq!(m("a**b", "a/c/b", FNM_PATHNAME), FNM_NOMATCH);
}

#[test]
fn a_long_literal_pattern() {
    let pattern = "x".repeat(1000);
    assert_eq!(
        fnmatch(pattern.as_bytes(), pattern.as_bytes(), 0, &Utf8Locale),
        0
    );
}

#[test]
fn a_long_star_pattern_is_fast() {
    let subject = "a".repeat(1000);
    let start = std::time::Instant::now();
    assert_eq!(m("*a*a*a*a*a*b", &subject, 0), FNM_NOMATCH);
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
}

#[test]
fn a_pathname_star_does_not_cross_a_slash() {
    assert_eq!(m("a/*/c", "a/b/c", FNM_PATHNAME), 0);
    assert_eq!(m("a/*/c", "a/b/d/c", FNM_PATHNAME), FNM_NOMATCH);
    assert_eq!(m("a/*/c", "a/b/d/c", 0), 0);
}

#[test]
fn a_bracket_slash_with_pathname() {
    assert_eq!(m("test[/]this", "test/this", FNM_PATHNAME), FNM_NOMATCH);
    assert_eq!(m("test[/]this", "test/this", 0), 0);
}
