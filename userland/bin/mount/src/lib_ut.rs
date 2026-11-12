// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `mount` pure logic.

use super::*;
use std::string::String;

#[test]
fn no_args_lists() {
    assert_eq!(parse_args([].into_iter()), Ok(Action::List));
}

#[test]
fn selftest_is_recognised() {
    assert_eq!(parse_args(["--selftest"].into_iter()), Ok(Action::Selftest));
}

#[test]
fn scheme_and_path_are_parsed() {
    assert_eq!(
        parse_args(["ramfs:", "/mnt"].into_iter()),
        Ok(Action::Mount {
            scheme: "ramfs:",
            path: "/mnt"
        })
    );
}

#[test]
fn bad_arguments_are_rejected() {
    assert_eq!(parse_args(["ramfs:"].into_iter()), Err(UsageError));
    assert_eq!(
        parse_args(["ramfs:", "/mnt", "x"].into_iter()),
        Err(UsageError)
    );
    assert_eq!(parse_args(["ramfs", "/mnt"].into_iter()), Err(UsageError));
    assert_eq!(parse_args(["ramfs:", "mnt"].into_iter()), Err(UsageError));
}

#[test]
fn schemes_and_paths_are_validated() {
    assert!(valid_scheme("ramfs:"));
    assert!(valid_scheme("net-tcp:"));
    assert!(!valid_scheme(""));
    assert!(!valid_scheme("ramfs"));
    assert!(!valid_scheme(":"));
    assert!(!valid_scheme("Ramfs:"));
    assert!(valid_path("/"));
    assert!(valid_path("/mnt"));
    assert!(!valid_path("mnt"));
    assert!(!valid_path("/a\0b"));
}

#[test]
fn mount_file_is_rendered_scheme_first() {
    let text = "# comment\n/ initfs:\n/tmp ramfs:\n\n/dev/null null:\n";
    let mut out = String::new();
    format_mounts(text, &mut out).unwrap();
    assert_eq!(out, "initfs: on /\nramfs: on /tmp\nnull: on /dev/null\n");
}
