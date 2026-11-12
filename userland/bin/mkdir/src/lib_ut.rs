// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `mkdir`.

use super::{parents, parse};
use alloc::string::String;
use alloc::vec;

#[test]
fn no_flags_by_default() {
    assert!(!parse([]).unwrap().flags.p);
}

#[test]
fn parent_flag_is_parsed() {
    assert!(parse(["-p"]).unwrap().flags.p);
}

#[test]
fn operands_are_kept() {
    assert_eq!(parse(["a", "b"]).unwrap().operands, vec!["a", "b"]);
}

#[test]
fn unknown_flag_is_rejected() {
    assert!(parse(["-z"]).is_err());
}

#[test]
fn one_component_has_one_parent() {
    assert_eq!(parents("/a"), vec!["/a"]);
}

#[test]
fn nested_path_lists_every_prefix() {
    assert_eq!(parents("/a/b/c"), vec!["/a", "/a/b", "/a/b/c"]);
}

#[test]
fn root_has_no_parents() {
    assert!(parents("/").is_empty());
}

#[test]
fn trailing_slash_is_ignored() {
    assert_eq!(parents("/a/b/"), vec!["/a", "/a/b"]);
}

#[test]
fn relative_path_has_no_parents() {
    assert!(parents("a/b").is_empty());
}

#[test]
fn parent_chain_owns_its_strings() {
    let chain: vec::Vec<String> = parents("/x");
    assert_eq!(chain.first().map(String::as_str), Some("/x"));
}
