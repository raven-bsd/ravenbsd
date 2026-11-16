// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the POSIX `sed` subset.

use super::{SedError, run};

#[test]
fn substitute_first_and_global() {
    assert_eq!(run("s/a/b/", "a a\n", false).unwrap(), "b a\n");
    assert_eq!(run("s/a/b/g", "a a\n", false).unwrap(), "b b\n");
}

#[test]
fn substitute_nth_and_print_flag() {
    assert_eq!(run("s/a/b/2", "a a a\n", false).unwrap(), "a b a\n");
    assert_eq!(run("s/a/b/p", "a\n", false).unwrap(), "b\nb\n");
}

#[test]
fn substitute_backreference_and_alternate_delimiter() {
    assert_eq!(run(r"s/\(a\)/[\1]/", "a\n", false).unwrap(), "[a]\n");
    assert_eq!(run("s|a|b|g", "a a\n", false).unwrap(), "b b\n");
}

#[test]
fn line_last_regex_and_range_addresses() {
    assert_eq!(run("2s/a/b/", "a\na\n", false).unwrap(), "a\nb\n");
    assert_eq!(run("$s/a/b/", "a\na\n", false).unwrap(), "a\nb\n");
    assert_eq!(run("/x/s/x/b/", "x\ny\n", false).unwrap(), "b\ny\n");
    assert_eq!(run("2,3s/a/b/", "a\na\na\n", false).unwrap(), "a\nb\nb\n");
}

#[test]
fn delete_quit_number_and_quiet() {
    assert_eq!(run("2d", "a\nb\nc\n", false).unwrap(), "a\nc\n");
    assert_eq!(run("2q", "a\nb\nc\n", false).unwrap(), "a\nb\n");
    assert_eq!(run("2=", "a\nb\n", false).unwrap(), "a\n2\nb\n");
    assert_eq!(run("p", "a\n", true).unwrap(), "a\n");
}

#[test]
fn translate_append_insert_change() {
    assert_eq!(run("y/abc/xyz/", "abc\n", false).unwrap(), "xyz\n");
    assert_eq!(run("1a\\\nafter", "a\n", false).unwrap(), "a\nafter\n");
    assert_eq!(run("1i\\\nbefore", "a\n", false).unwrap(), "before\na\n");
    assert_eq!(
        run("1c\\\nchanged", "a\nb\n", false).unwrap(),
        "changed\nb\n"
    );
}

#[test]
fn multiple_commands_and_address_negation() {
    assert_eq!(run("s/a/b/;s/b/c/", "a\n", false).unwrap(), "c\n");
    assert_eq!(run("2!s/a/b/", "a\na\n", false).unwrap(), "b\na\n");
}

#[test]
fn invalid_script_is_rejected() {
    assert_eq!(run("s/a", "a\n", false), Err(SedError::Usage));
    assert_eq!(run("y/ab/c/", "ab\n", false), Err(SedError::Usage));
    assert_eq!(run("s/[a/b/", "a\n", false), Err(SedError::Regex));
}
