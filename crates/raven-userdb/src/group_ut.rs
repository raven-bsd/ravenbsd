// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `/etc/group` records.

use super::*;

#[test]
fn a_group_round_trips() {
    let line = "wheel:x:0:root";
    let group = GroupEntry::parse(line, 1).expect("wheel");
    assert_eq!(group.name, "wheel");
    assert_eq!(group.gid, 0);
    assert!(group.contains("root"));
    assert_eq!(group.render(), line);
}

#[test]
fn an_empty_member_list_is_valid() {
    let group = GroupEntry::parse("users:x:100:", 2).expect("users");
    assert!(group.members.is_empty());
    assert_eq!(group.render(), "users:x:100:");
}

#[test]
fn members_are_added_once_and_removed() {
    let mut group = GroupEntry::parse("users:x:100:", 2).expect("users");
    assert!(group.add_member("alice"));
    assert!(!group.add_member("alice"));
    assert!(group.contains("alice"));
    assert!(group.remove_member("alice"));
    assert!(!group.remove_member("alice"));
}

#[test]
fn member_names_must_be_valid_and_unique() {
    for line in [
        "users:x:100:alice,alice",
        "users:x:100:bad name",
        "users:x:100::alice",
    ] {
        assert!(GroupEntry::parse(line, 1).is_err(), "{line}");
    }
}

#[test]
fn the_password_field_is_x_or_star() {
    assert!(GroupEntry::parse("users:x:100:", 1).is_ok());
    assert!(GroupEntry::parse("users:*:100:", 1).is_ok());
    assert!(GroupEntry::parse("users:secret:100:", 1).is_err());
}
