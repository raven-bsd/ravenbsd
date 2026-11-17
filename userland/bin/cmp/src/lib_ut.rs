// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `cmp`.

use super::{Difference, differences, first_difference, format_difference, parse};

#[test]
fn parse_flags_and_operands() {
    let options = parse(["-l", "-s", "a", "b"]).unwrap();
    assert!(options.list);
    assert!(options.silent);
    assert_eq!(options.operands, vec!["a", "b"]);
}

#[test]
fn unknown_option_is_rejected() {
    assert!(parse(["-x"]).is_err());
}

#[test]
fn equal_inputs_have_no_difference() {
    assert_eq!(first_difference(b"abc", b"abc"), None);
    assert!(differences(b"abc", b"abc").is_empty());
}

#[test]
fn first_difference_is_reported() {
    assert_eq!(first_difference(b"abc", b"axc"), Some(1));
    assert_eq!(first_difference(b"abc", b"ab"), Some(2));
}

#[test]
fn list_reports_every_difference() {
    let result = differences(b"abc", b"axz");
    assert_eq!(result.len(), 2);
    assert_eq!(
        format_difference(&Difference {
            offset: 1,
            left: Some(b'b'),
            right: Some(b'x'),
        }),
        "2 142 170"
    );
}
