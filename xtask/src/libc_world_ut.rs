// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn no_arguments_request_usage() {
    assert!(wants_usage(&[]));
}

#[test]
fn help_flags_request_usage() {
    assert!(wants_usage(&args(&["--help"])));
    assert!(wants_usage(&args(&["-h"])));
}

#[test]
fn a_command_does_not_request_usage() {
    assert!(!wants_usage(&args(&["list"])));
    assert!(!wants_usage(&args(&["build", "--all"])));
}

#[test]
fn the_forwarded_vector_starts_with_the_cargo_invocation() {
    let vector = forward_args(&args(&["list"]));
    assert_eq!(vector[..5], ["run", "-q", "-p", "libc-world", "--"]);
    assert_eq!(vector[5], "list");
}

#[test]
fn every_argument_is_forwarded_in_order() {
    let vector = forward_args(&args(&["build", "--subset", "smoke5", "--jobs", "2"]));
    assert_eq!(vector[5..], ["build", "--subset", "smoke5", "--jobs", "2"]);
}

#[test]
fn exit_codes_are_preserved() {
    assert_eq!(exit_code(0), 0);
    assert_eq!(exit_code(1), 1);
    assert_eq!(exit_code(2), 2);
    assert_eq!(exit_code(77), 77);
}

#[test]
fn a_negative_exit_code_becomes_two() {
    assert_eq!(exit_code(-1), 2);
}
