// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the `libc-size` forwarder (no subprocess is spawned).

use super::*;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn command_is_a_cargo_run_with_a_separator() {
    let command = command_for(&args(&["check"]));
    assert_eq!(command, vec!["run", "-q", "-p", "libc-size", "--", "check"]);
}

#[test]
fn every_argument_is_forwarded_verbatim() {
    let command = command_for(&args(&[
        "run",
        "--arch",
        "x86_64",
        "--linkage",
        "static",
        "--set",
        "tiny",
    ]));
    assert_eq!(
        command,
        vec![
            "run",
            "-q",
            "-p",
            "libc-size",
            "--",
            "run",
            "--arch",
            "x86_64",
            "--linkage",
            "static",
            "--set",
            "tiny"
        ]
    );
}

#[test]
fn check_shorthand_is_split_from_the_run_flags() {
    let (forwarded, check) = split_check(&args(&["run", "--arch", "x86_64", "--check"]));
    assert!(check);
    assert_eq!(forwarded, vec!["run", "--arch", "x86_64"]);
}

#[test]
fn no_check_means_no_split() {
    let (forwarded, check) = split_check(&args(&["check"]));
    assert!(!check);
    assert_eq!(forwarded, vec!["check"]);
}

#[test]
fn an_empty_invocation_defaults_to_run() {
    let (forwarded, check) = split_check(&args(&[]));
    assert!(forwarded.is_empty());
    assert!(!check);
}

#[test]
fn check_only_invocation_keeps_no_run_flags() {
    let (forwarded, check) = split_check(&args(&["--check"]));
    assert!(check);
    assert!(forwarded.is_empty());
}

#[test]
fn original_arguments_are_not_mutated() {
    let original = args(&["run", "--check"]);
    let _ = split_check(&original);
    assert_eq!(original, vec!["run", "--check"]);
}

#[test]
fn flags_without_a_command_get_the_implicit_run() {
    assert_eq!(
        forward_args(&args(&["--arch", "x86_64", "--set", "tiny"])),
        vec!["run", "--arch", "x86_64", "--set", "tiny"]
    );
}

#[test]
fn an_explicit_command_is_kept() {
    assert_eq!(forward_args(&args(&["check"])), vec!["check"]);
    assert_eq!(
        forward_args(&args(&["run", "--arch", "x86_64"])),
        vec!["run", "--arch", "x86_64"]
    );
}

#[test]
fn an_empty_forward_gets_run() {
    assert_eq!(forward_args(&[]), vec!["run"]);
}

#[test]
fn unknown_flags_are_forwarded_not_rejected() {
    let (forwarded, _) = split_check(&args(&["run", "--no-build-libc"]));
    assert_eq!(forwarded, vec!["run", "--no-build-libc"]);
}
