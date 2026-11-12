// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `cp`.

use super::{destination, pair, parse};

#[test]
fn recursive_flag_is_parsed() {
    assert!(parse(["-r"]).unwrap().flags.r);
}

#[test]
fn unknown_flag_is_rejected() {
    assert!(parse(["-a"]).is_err());
}

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
fn double_dash_allows_a_dash_name() {
    assert_eq!(parse(["--", "-r"]).unwrap().operands, vec!["-r"]);
}
