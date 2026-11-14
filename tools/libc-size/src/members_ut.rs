// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for wildcard matching and member classification.

use super::*;
use crate::measure::Berkeley;

fn member(name: &str, text: u64) -> MemberSize {
    MemberSize {
        name: name.to_string(),
        size: Berkeley {
            text,
            data: 0,
            bss: 0,
        },
    }
}

#[test]
fn wildcard_matches_at_both_ends() {
    assert!(wildcard_search("*strlen*", "raven_libc_cgu.strlen.o"));
    assert!(wildcard_search("strlen*", "strlen"));
    assert!(wildcard_search("*len", "strlen"));
    assert!(wildcard_search("*", "anything"));
}

#[test]
fn wildcard_without_star_is_a_contains() {
    assert!(wildcard_search("strlen", "libc.a(strlen.o)"));
    assert!(!wildcard_search("strnlen", "strlen"));
}

#[test]
fn wildcard_empty_pattern_matches() {
    assert!(wildcard_search("", "anything"));
}

#[test]
fn wildcard_does_not_match_missing_text() {
    assert!(!wildcard_search("regex*", "raven_libc_stdio-cgu.1.o"));
    assert!(!wildcard_search("*fmt_float*", "raven_libc_dtoa.cgu.o"));
}

#[test]
fn shell_cgu_names_expose_their_crate() {
    let candidates = member_candidates("raven_libc_regex-a864.raven_libc_regex.ffd-cgu.02.rcgu.o");
    assert!(candidates.iter().any(|value| value == "regex"));
    assert!(candidates.iter().any(|value| value == "raven_libc_regex"));
}

#[test]
fn rule_patterns_match_real_member_shapes() {
    let members = vec![
        "raven_libc_regex-a864.raven_libc_regex.ffd-cgu.02.rcgu.o".to_string(),
        "raven_libc-8282.raven_libc.78-cgu.017.rcgu.o".to_string(),
        "core-1afd.core.674-cgu.0.rcgu.o".to_string(),
    ];
    let patterns = vec!["regex*".to_string()];
    let matched = matching_members(&members, &patterns);
    assert_eq!(matched.len(), 1);
    assert!(matched.first().expect("one").contains("raven_libc_regex"));
}

#[test]
fn stdio_pattern_matches_stdio_and_not_stdlib() {
    let members = vec![
        "raven_libc_stdio-0ada.raven_libc_stdio.d4-cgu.0.rcgu.o".to_string(),
        "raven_libc_stdlib-1.raven_libc_stdlib.2-cgu.0.rcgu.o".to_string(),
    ];
    let matched = matching_members(&members, &["stdio*".to_string()]);
    assert_eq!(matched.len(), 1);
    assert!(matched.first().expect("one").contains("raven_libc_stdio"));
}

#[test]
fn fmt_float_pattern_does_not_match_fmt_core() {
    let members = vec![
        "raven_libc_fmt-1.raven_libc_fmt.2-cgu.0.rcgu.o".to_string(),
        "raven_libc_fmt_float-1.raven_libc_fmt_float.2-cgu.0.rcgu.o".to_string(),
    ];
    let matched = matching_members(&members, &["fmt_float*".to_string()]);
    assert_eq!(matched.len(), 1);
    assert!(matched.first().expect("one").contains("fmt_float"));
}

#[test]
fn symbol_matching_searches_the_mangled_name() {
    assert!(symbol_matches(
        "_RINvNtCs1234_4core3fmt9Arguments6new_v1",
        &["core3fmt*".to_string()]
    ));
    assert!(symbol_matches("__adddf3", &["__*df3".to_string()]));
    assert!(symbol_matches("__addsf3", &["__*sf3".to_string()]));
    assert!(!symbol_matches("__adddf3", &["__*sf3".to_string()]));
}

#[test]
fn large_members_are_sorted_and_filtered() {
    let members = vec![
        member("small.o", 100),
        member("big.o", 5000),
        member("bigger.o", 9000),
    ];
    let large = large_members(&members, 4096);
    assert_eq!(large.len(), 2);
    assert_eq!(large.first().expect("first").name, "bigger.o");
    assert_eq!(large.get(1).expect("second").name, "big.o");
}

#[test]
fn render_table_has_one_line_per_member() {
    let table = render_table(&[member("a.o", 1), member("b.o", 2)]);
    assert_eq!(table.lines().count(), 2);
    assert!(table.contains("a.o 1 0 0"));
}
