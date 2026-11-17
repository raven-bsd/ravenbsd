// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `getconf` table and parser.

use super::*;

#[test]
fn known_values_are_the_documented_ones() {
    assert_eq!(lookup("ARG_MAX"), Some("131072"));
    assert_eq!(lookup("PAGESIZE"), Some("4096"));
    assert_eq!(lookup("PAGE_SIZE"), Some("4096"));
    assert_eq!(lookup("_POSIX_VERSION"), Some("202405"));
}

#[test]
fn unknown_values_are_not_invented() {
    assert_eq!(lookup("NO_SUCH_VARIABLE"), None);
}

#[test]
fn parses_one_and_many_names() {
    let one = parse_args(["ARG_MAX"]).unwrap();
    assert_eq!(one.names, vec!["ARG_MAX".to_string()]);
    let many = parse_args(["ARG_MAX", "PAGESIZE", "_POSIX_VERSION"]).unwrap();
    assert_eq!(
        many.names,
        vec![
            "ARG_MAX".to_string(),
            "PAGESIZE".to_string(),
            "_POSIX_VERSION".to_string()
        ]
    );
}

#[test]
fn parses_all() {
    let options = parse_args(["-a"]).unwrap();
    assert!(options.all);
    assert!(options.names.is_empty());
}

#[test]
fn unknown_names_are_kept_for_the_main_error_path() {
    assert_eq!(
        parse_args(["NOPE"]).unwrap().names,
        vec!["NOPE".to_string()]
    );
    assert_eq!(parse_args(["--bogus"]), Err(UsageError::UnknownOption));
    assert_eq!(parse_args(Vec::<&str>::new()), Err(UsageError::MissingName));
}

#[test]
fn all_line_is_name_space_value() {
    let line = all_line(&Entry {
        name: "PAGESIZE",
        value: "4096",
    });
    assert_eq!(line, "PAGESIZE 4096");
}
