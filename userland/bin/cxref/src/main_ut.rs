// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the cxref argument parser.

use super::*;

fn args(values: &[&str]) -> std::vec::IntoIter<String> {
    values
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>()
        .into_iter()
}

#[test]
fn defaults_to_stdout() {
    let options = parse_args(args(&["a.c"])).expect("options");
    assert_eq!(options.output, None);
    assert_eq!(options.files, vec!["a.c".to_string()]);
}

#[test]
fn accepts_xref_and_output() {
    let options = parse_args(args(&["-x", "-o", "table", "a.c"])).expect("options");
    assert_eq!(options.output.as_deref(), Some("table"));
    assert_eq!(options.files, vec!["a.c".to_string()]);
}

#[test]
fn rejects_missing_input_and_bad_options() {
    assert!(parse_args(args(&[])).is_err());
    assert!(parse_args(args(&["-q", "a.c"])).is_err());
    assert!(parse_args(args(&["-o"])).is_err());
}
