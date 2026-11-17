// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the ctags argument parser.

use super::*;

fn args(values: &[&str]) -> std::vec::IntoIter<String> {
    values
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>()
        .into_iter()
}

#[test]
fn defaults_to_a_tags_file() {
    let options = parse_args(args(&["a.c"])).expect("options");
    assert_eq!(options.output, "tags");
    assert!(!options.xref);
    assert_eq!(options.files, vec!["a.c".to_string()]);
}

#[test]
fn parses_output_and_xref() {
    let options = parse_args(args(&["-x", "-f", "out", "a.c", "b.c"])).expect("options");
    assert!(options.xref);
    assert_eq!(options.output, "out");
    assert_eq!(options.files, vec!["a.c".to_string(), "b.c".to_string()]);
}

#[test]
fn rejects_missing_input_and_bad_options() {
    assert!(parse_args(args(&[])).is_err());
    assert!(parse_args(args(&["-q", "a.c"])).is_err());
    assert!(parse_args(args(&["-f"])).is_err());
}
