// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `whoami` argument and name handling.

use super::*;

const PASSWD: &str = include_str!("../../../../tests/fixtures/user-management/passwd");
const GROUP: &str = include_str!("../../../../tests/fixtures/user-management/group");

fn database() -> Database {
    Database::parse_lookup(PASSWD, GROUP).expect("fixture")
}

#[test]
fn there_are_no_operands() {
    assert!(parse_args([]).is_ok());
    assert_eq!(
        parse_args(["root"]).expect_err("extra"),
        UsageError::ExtraOperand
    );
}

#[test]
fn known_ids_get_names() {
    let database = database();
    assert_eq!(name_for_uid(&database, 0), "root");
    assert_eq!(name_for_uid(&database, 1000), "alice");
    assert!(known_uid(&database, 0));
}

#[test]
fn unknown_ids_stay_numeric() {
    let database = database();
    assert_eq!(name_for_uid(&database, 4242), "4242");
    assert!(!known_uid(&database, 4242));
}
