// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the credential and permission matrix.

use super::*;

fn user(uid: u32, gid: u32) -> Credentials {
    Credentials::new(uid, uid, gid, gid, [0; MAX_GROUPS], 0, 0)
}

#[test]
fn root_bypasses_read_and_write_but_not_exec() {
    let root = Credentials::ROOT;
    assert!(root.permitted(0o000, 1000, 1000, ACCESS_READ));
    assert!(root.permitted(0o000, 1000, 1000, ACCESS_WRITE));
    assert!(!root.permitted(0o000, 1000, 1000, ACCESS_EXEC));
    assert!(root.permitted(0o001, 1000, 1000, ACCESS_EXEC));
    assert!(root.permitted(0o100, 1000, 1000, ACCESS_EXEC));
    assert!(root.permitted(0o010, 1000, 1000, ACCESS_EXEC));
}

#[test]
fn owner_group_and_other_classes_are_selected() {
    let owner = user(1000, 1000);
    assert!(owner.permitted(0o600, 1000, 1000, ACCESS_READ));
    assert!(owner.permitted(0o600, 1000, 1000, ACCESS_WRITE));
    assert!(!owner.permitted(0o600, 1000, 1000, ACCESS_EXEC));
    assert!(!owner.permitted(0o400, 1000, 1000, ACCESS_WRITE));

    let group = Credentials::new(
        1001,
        1001,
        1001,
        1001,
        [10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        1,
        0,
    );
    assert!(group.in_group(10));
    assert!(group.permitted(0o040, 1000, 10, ACCESS_READ));
    assert!(!group.permitted(0o040, 1000, 10, ACCESS_WRITE));

    let other = user(1002, 1002);
    assert!(other.permitted(0o004, 1000, 1000, ACCESS_READ));
    assert!(other.permitted(0o002, 1000, 1000, ACCESS_WRITE));
    assert!(other.permitted(0o001, 1000, 1000, ACCESS_EXEC));
    assert!(!other.permitted(0o600, 1000, 1000, ACCESS_READ));
}

#[test]
fn group_count_is_clamped_and_group_list_is_bounded() {
    let mut groups = [0u32; MAX_GROUPS];
    groups[0] = 7;
    let creds = Credentials::new(1, 1, 1, 1, groups, u32::MAX, 0);
    assert_eq!(creds.group_count as usize, MAX_GROUPS);
    assert_eq!(creds.group_list().len(), MAX_GROUPS);
    assert!(creds.in_group(7));
}

#[test]
fn sticky_delete_allows_owner_or_root_only() {
    let owner = user(1000, 1000);
    assert!(owner.sticky_delete_allowed(0o1777, 1000));
    assert!(!owner.sticky_delete_allowed(0o1777, 1001));
    assert!(owner.sticky_delete_allowed(0o777, 1001));
    assert!(Credentials::ROOT.sticky_delete_allowed(0o1777, 1001));
}

#[test]
fn execute_check_requires_the_execute_bit() {
    let owner = user(1000, 1000);
    assert!(!owner.can_exec(0o644, 1000, 1000));
    assert!(owner.can_exec(0o755, 1000, 1000));
    assert!(!Credentials::ROOT.can_exec(0o644, 1000, 1000));
    assert!(Credentials::ROOT.can_exec(0o755, 1000, 1000));
}
