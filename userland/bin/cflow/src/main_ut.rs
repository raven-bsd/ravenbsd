// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the cflow argument parser.

use super::*;

fn args(values: &[&str]) -> std::vec::IntoIter<String> {
    values
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>()
        .into_iter()
}

#[test]
fn defaults_to_stdout_and_forward_order() {
    let options = parse_args(args(&["a.c"])).expect("options");
    assert_eq!(options.output, None);
    assert!(!options.reverse);
    assert_eq!(options.files, vec!["a.c".to_string()]);
}

#[test]
fn parses_reverse_and_output() {
    let options = parse_args(args(&["-r", "-o", "graph", "a.c"])).expect("options");
    assert!(options.reverse);
    assert_eq!(options.output.as_deref(), Some("graph"));
    assert_eq!(options.files, vec!["a.c".to_string()]);
}

#[test]
fn rejects_missing_input_and_bad_options() {
    assert!(parse_args(args(&[])).is_err());
    assert!(parse_args(args(&["-q", "a.c"])).is_err());
    assert!(parse_args(args(&["-o"])).is_err());
}
