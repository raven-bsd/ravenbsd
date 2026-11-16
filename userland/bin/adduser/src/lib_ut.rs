// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `adduser` defaults and argument handling.

use super::*;

#[test]
fn the_name_is_optional() {
    assert_eq!(parse_args([]).expect("none"), None);
    assert_eq!(parse_args(["alice"]).expect("name"), Some("alice"));
}

#[test]
fn extra_operands_and_options_are_rejected() {
    assert_eq!(
        parse_args(["alice", "bob"]).expect_err("extra"),
        UsageError::ExtraOperand
    );
    assert_eq!(
        parse_args(["-m"]).expect_err("option"),
        UsageError::UnknownOption('m')
    );
}

#[test]
fn the_default_home_is_below_home() {
    assert_eq!(default_home("alice"), "/home/alice");
    assert_eq!(DEFAULT_SHELL, "/bin/brush");
}
