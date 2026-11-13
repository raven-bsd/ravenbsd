// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the universe builder.

use super::*;
use crate::testutil::TempDir;

#[test]
fn build_sorts_and_deduplicates() {
    let universe = Universe::build([
        ("zebra".to_string(), Kind::Func),
        ("alpha".to_string(), Kind::Func),
        ("zebra".to_string(), Kind::Func),
    ]);
    assert_eq!(universe.names(), vec!["alpha", "zebra"]);
}

#[test]
fn build_filters_short_and_non_ascii_names() {
    let universe = Universe::build([
        ("x".to_string(), Kind::Func),
        ("-bad".to_string(), Kind::Func),
        ("a b".to_string(), Kind::Func),
        ("ok_1".to_string(), Kind::Func),
        ("é".to_string(), Kind::Func),
    ]);
    assert_eq!(universe.names(), vec!["ok_1"]);
}

#[test]
fn data_kind_wins_over_func() {
    let universe = Universe::build([
        ("environ".to_string(), Kind::Func),
        ("environ".to_string(), Kind::Data),
        ("errno".to_string(), Kind::Macro),
    ]);
    assert_eq!(universe.kind("environ"), Some(Kind::Data));
    assert_eq!(universe.kind("errno"), Some(Kind::Macro));
    assert_eq!(universe.kind("missing"), None);
}

#[test]
fn read_names_file_ignores_comments_and_blanks() {
    let temp = TempDir::new("universe-names");
    temp.write("names.txt", "# comment\n\nalpha\n beta \nnot a name\n");
    let names = read_names_file(&temp.path().join("names.txt")).unwrap();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn extend_merges_and_keeps_the_stronger_kind() {
    let mut left = Universe::build([("one".to_string(), Kind::Func)]);
    let right = Universe::build([
        ("one".to_string(), Kind::Data),
        ("two".to_string(), Kind::Func),
    ]);
    left.extend(right);
    assert_eq!(left.names(), vec!["one", "two"]);
    assert_eq!(left.kind("one"), Some(Kind::Data));
}

#[test]
fn identifier_rule_is_ascii_c() {
    assert!(is_identifier("strlcpy"));
    assert!(is_identifier("__x"));
    assert!(!is_identifier("1x"));
    assert!(!is_identifier("x"));
    assert!(!is_identifier("a-b"));
}
