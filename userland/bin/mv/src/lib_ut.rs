// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `mv`.

use super::{destination, pair, parse};

#[test]
fn exactly_two_operands_are_a_pair() {
    assert_eq!(pair(&["a", "b"]), Some(("a", "b")));
}

#[test]
fn one_operand_is_not_a_pair() {
    assert_eq!(pair(&["a"]), None);
}

#[test]
fn three_operands_are_not_a_pair() {
    assert_eq!(pair(&["a", "b", "c"]), None);
}

#[test]
fn no_operands_is_not_a_pair() {
    assert_eq!(pair(&[]), None);
}

#[test]
fn file_target_is_used_verbatim() {
    assert_eq!(destination("/tmp/a", "/tmp/b", false), "/tmp/b");
}

#[test]
fn directory_target_gets_the_basename() {
    assert_eq!(destination("/tmp/a", "/dest", true), "/dest/a");
}

#[test]
fn root_target_gets_the_basename() {
    assert_eq!(destination("/tmp/a", "/", true), "/a");
}

#[test]
fn options_are_rejected() {
    assert!(parse(["-f"]).is_err());
}

#[test]
fn double_dash_allows_a_dash_name() {
    assert_eq!(parse(["--", "-f"]).unwrap().operands, vec!["-f"]);
}
