// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `renice` parser.

use super::*;
use alloc::vec;

#[test]
fn parses_bare_and_n_priority_forms() {
    let bare = parse_args(["5", "1", "2"]).unwrap();
    assert_eq!(bare.priority, 5);
    assert_eq!(bare.which, Which::Process);
    assert_eq!(bare.ids, vec![1, 2]);
    let explicit = parse_args(["-n", "-5", "7"]).unwrap();
    assert_eq!(explicit.priority, -5);
    assert_eq!(explicit.ids, vec![7]);
}

#[test]
fn parses_group_and_user_selectors() {
    let group = parse_args(["-n", "5", "-g", "1"]).unwrap();
    assert_eq!(group.which, Which::Group);
    assert_eq!(group.ids, vec![1]);
    let user = parse_args(["-u", "5", "0"]).unwrap();
    assert_eq!(user.which, Which::User);
    assert_eq!(user.ids, vec![0]);
}

#[test]
fn selector_after_priority_is_honoured() {
    let options = parse_args(["5", "-g", "1"]).unwrap();
    assert_eq!(options.priority, 5);
    assert_eq!(options.which, Which::Group);
    assert_eq!(options.ids, vec![1]);
}

#[test]
fn rejects_bad_and_missing_values() {
    assert_eq!(parse_args(["-n"]), Err(UsageError::MissingValue));
    assert_eq!(parse_args(["nope", "1"]), Err(UsageError::BadPriority));
    assert_eq!(parse_args(["5", "nope"]), Err(UsageError::BadId));
    assert_eq!(parse_args(["5"]), Err(UsageError::MissingIds));
    assert_eq!(parse_args(["-x"]), Err(UsageError::UnknownOption));
}
