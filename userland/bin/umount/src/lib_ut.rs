// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `umount` pure logic.

use super::*;

#[test]
fn absolute_path_is_parsed() {
    assert_eq!(parse_args(["/mnt"].into_iter()), Ok(Action::Path("/mnt")));
}

#[test]
fn selftest_is_recognised() {
    assert_eq!(parse_args(["--selftest"].into_iter()), Ok(Action::Selftest));
}

#[test]
fn missing_and_extra_arguments_are_rejected() {
    assert_eq!(parse_args([].into_iter()), Err(UsageError));
    assert_eq!(parse_args(["/mnt", "x"].into_iter()), Err(UsageError));
    assert_eq!(parse_args(["--selftest", "x"].into_iter()), Err(UsageError));
}

#[test]
fn relative_paths_are_rejected() {
    assert_eq!(parse_args(["mnt"].into_iter()), Err(UsageError));
    assert_eq!(parse_args([""].into_iter()), Err(UsageError));
}

#[test]
fn paths_are_bounded_and_printable() {
    assert!(valid_path("/"));
    assert!(valid_path("/mnt"));
    assert!(!valid_path("mnt"));
    assert!(!valid_path("/a\0b"));
    assert!(!valid_path(&format!("/{}", "a".repeat(256))));
}
