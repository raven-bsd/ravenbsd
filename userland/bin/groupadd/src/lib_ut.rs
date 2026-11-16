// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `groupadd` argument handling.

use super::*;

#[test]
fn a_plain_name_is_allocated() {
    let options = parse_args(["staff"]).expect("parse");
    assert_eq!(options.name, "staff");
    assert_eq!(options.gid, None);
    assert!(!options.system);
}

#[test]
fn explicit_and_system_ids_are_parsed() {
    let options = parse_args(["-r", "-g", "250", "staff"]).expect("parse");
    assert!(options.system);
    assert_eq!(options.gid, Some(250));
    assert_eq!(
        parse_args(["-g300", "staff"]).expect("attached").gid,
        Some(300)
    );
}

#[test]
fn operand_errors_are_reported() {
    assert_eq!(
        parse_args([]).expect_err("missing"),
        UsageError::MissingName
    );
    assert_eq!(
        parse_args(["a", "b"]).expect_err("extra"),
        UsageError::ExtraOperand
    );
}

#[test]
fn value_errors_are_reported() {
    assert_eq!(
        parse_args(["-g"]).expect_err("value"),
        UsageError::MissingValue('g')
    );
    assert_eq!(
        parse_args(["-g", "ten", "staff"]).expect_err("gid"),
        UsageError::BadValue('g')
    );
    assert_eq!(
        parse_args(["-z", "staff"]).expect_err("unknown"),
        UsageError::UnknownOption('z')
    );
}
