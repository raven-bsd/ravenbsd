// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `useradd` argument handling.

use super::*;

#[test]
fn every_option_is_parsed() {
    let options = parse_args([
        "-m",
        "-d",
        "/srv/alice",
        "-s",
        "/bin/sh",
        "-u",
        "1500",
        "-g",
        "users",
        "-G",
        "wheel,staff",
        "-c",
        "Alice Example",
        "alice",
    ])
    .expect("parse");
    assert!(options.create_home);
    assert_eq!(options.home, Some("/srv/alice"));
    assert_eq!(options.shell, Some("/bin/sh"));
    assert_eq!(options.uid, Some(1500));
    assert_eq!(options.group, Some("users"));
    assert_eq!(options.groups, ["wheel", "staff"]);
    assert_eq!(options.gecos, Some("Alice Example"));
    assert_eq!(options.name, "alice");
}

#[test]
fn attached_values_are_accepted() {
    let options = parse_args(["-u1000", "-Gusers,wheel", "--", "-dave"]).expect("parse");
    assert_eq!(options.uid, Some(1000));
    assert_eq!(options.groups, ["users", "wheel"]);
    assert_eq!(options.name, "-dave");
}

#[test]
fn a_name_is_required_and_there_is_only_one() {
    assert_eq!(
        parse_args([]).expect_err("missing"),
        UsageError::MissingName
    );
    assert_eq!(
        parse_args(["alice", "bob"]).expect_err("extra"),
        UsageError::ExtraOperand
    );
}

#[test]
fn a_missing_option_value_is_reported() {
    assert_eq!(
        parse_args(["-d"]).expect_err("value"),
        UsageError::MissingValue('d')
    );
}

#[test]
fn a_bad_uid_is_reported() {
    assert_eq!(
        parse_args(["-u", "ten", "alice"]).expect_err("uid"),
        UsageError::BadValue('u')
    );
}

#[test]
fn unknown_options_are_reported() {
    assert_eq!(
        parse_args(["-z", "alice"]).expect_err("unknown"),
        UsageError::UnknownOption('z')
    );
    assert_eq!(
        parse_args(["-mm", "alice"]).expect_err("combined"),
        UsageError::UnknownOption('m')
    );
}

#[test]
fn a_lone_dash_is_the_name_operand() {
    assert_eq!(parse_args(["-"]).expect("lone").name, "-");
}
