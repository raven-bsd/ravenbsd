// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `env` parser and environment merge.

use super::*;
use alloc::vec;

fn base() -> Vec<String> {
    vec![
        "PATH=/bin:/sbin".to_string(),
        "HOME=/".to_string(),
        "USER=root".to_string(),
    ]
}

#[test]
fn parses_assignments_and_command() {
    let options = parse_args(["FOO=bar", "BAZ=qux", "/bin/whoami"]).unwrap();
    assert!(!options.ignore);
    assert_eq!(
        options.assignments,
        vec![
            ("FOO".to_string(), "bar".to_string()),
            ("BAZ".to_string(), "qux".to_string())
        ]
    );
    assert_eq!(options.command, vec!["/bin/whoami"]);
}

#[test]
fn parses_ignore_and_arguments_after_command() {
    let options = parse_args(["-i", "FOO=bar", "/bin/echo", "-n", "hi"]).unwrap();
    assert!(options.ignore);
    assert_eq!(options.command, vec!["/bin/echo", "-n", "hi"]);
}

#[test]
fn command_without_assignments_is_not_an_option() {
    let options = parse_args(["/bin/echo", "-n"]).unwrap();
    assert!(options.command == vec!["/bin/echo", "-n"]);
}

#[test]
fn double_dash_stops_assignment_parsing() {
    let options = parse_args(["-i", "--", "A=B"]).unwrap();
    assert!(options.assignments.is_empty());
    assert_eq!(options.command, vec!["A=B"]);
}

#[test]
fn merge_replaces_and_appends() {
    let merged = merge_environment(
        &base(),
        false,
        &[
            ("USER".to_string(), "alice".to_string()),
            ("FOO".to_string(), "bar".to_string()),
        ],
    );
    assert_eq!(value(&merged, "USER"), Some("alice"));
    assert_eq!(value(&merged, "FOO"), Some("bar"));
    assert_eq!(merged.len(), 4);
    let ignored = merge_environment(&base(), true, &[("FOO".to_string(), "bar".to_string())]);
    assert_eq!(ignored, vec!["FOO=bar"]);
}

#[test]
fn candidates_use_path_and_absolute_commands() {
    let paths = candidates("whoami", &base());
    assert_eq!(
        paths,
        vec!["/bin/whoami".to_string(), "/sbin/whoami".to_string()]
    );
    assert_eq!(candidates("/bin/whoami", &base()), vec!["/bin/whoami"]);
}

#[test]
fn malformed_arguments_are_rejected() {
    assert_eq!(parse_args(["-x"]), Err(UsageError::UnknownOption));
    assert_eq!(parse_args(["=x"]), Err(UsageError::BadAssignment));
}
