// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `strings`.

use super::{extract, parse};

#[test]
fn parse_minimum_and_files() {
    let options = parse(["-a", "-n", "6", "a", "b"]).unwrap();
    assert!(options.all);
    assert_eq!(options.min, 6);
    assert_eq!(options.operands, vec!["a", "b"]);
}

#[test]
fn parse_defaults() {
    let options = parse(["file"]).unwrap();
    assert_eq!(options.min, 4);
    assert_eq!(options.operands, vec!["file"]);
}

#[test]
fn invalid_minimum_is_rejected() {
    assert!(parse(["-n", "x"]).is_err());
    assert!(parse(["-x"]).is_err());
}

#[test]
fn printable_runs_are_extracted() {
    assert_eq!(
        extract(b"\0abc\0defgh\0", 3),
        vec!["abc".to_string(), "defgh".to_string()]
    );
    assert_eq!(extract(b"\0abc\0defgh\0", 5), vec!["defgh".to_string()]);
}

#[test]
fn carriage_return_ends_a_run() {
    assert_eq!(
        extract(b"ab\rcd\n", 2),
        vec!["ab".to_string(), "cd".to_string()]
    );
}
