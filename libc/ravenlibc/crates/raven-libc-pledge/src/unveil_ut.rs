// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the unveil table.

use super::*;

fn perms(text: &[u8]) -> UnveilPerms {
    super::parse_perms(text).expect("permissions")
}

#[test]
fn an_unset_table_allows_everything() {
    let table = UnveilTable::new();
    assert_eq!(
        table.check(b"/anything", UnveilPerms(UnveilPerms::R)),
        Ok(())
    );
    assert_eq!(table.state(), UnveilState::Unset);
}

#[test]
fn the_first_call_activates_the_table() {
    let mut table = UnveilTable::new();
    table.unveil(Some(b"/a"), Some(b"r")).expect("unveil");
    assert_eq!(table.state(), UnveilState::Active);
    assert_eq!(
        table.check(b"/other", UnveilPerms(UnveilPerms::R)),
        Err(ENOENT)
    );
}

#[test]
fn an_exact_path_is_visible() {
    let mut table = UnveilTable::new();
    table.unveil(Some(b"/a"), Some(b"rw")).expect("unveil");
    assert_eq!(table.check(b"/a", perms(b"r")), Ok(()));
    assert_eq!(table.check(b"/a", perms(b"w")), Ok(()));
}

#[test]
fn a_missing_permission_is_eacces() {
    let mut table = UnveilTable::new();
    table.unveil(Some(b"/a"), Some(b"r")).expect("unveil");
    assert_eq!(table.check(b"/a", perms(b"w")), Err(EACCES));
}

#[test]
fn a_directory_prefix_is_covered() {
    let mut table = UnveilTable::new();
    table.unveil(Some(b"/a"), Some(b"r")).expect("unveil");
    assert_eq!(table.check(b"/a/b/c", perms(b"r")), Ok(()));
}

#[test]
fn a_string_prefix_is_not_covered() {
    let mut table = UnveilTable::new();
    table.unveil(Some(b"/a"), Some(b"r")).expect("unveil");
    assert_eq!(table.check(b"/ab", perms(b"r")), Err(ENOENT));
}

#[test]
fn the_longest_entry_wins() {
    let mut table = UnveilTable::new();
    table.unveil(Some(b"/a"), Some(b"r")).expect("a");
    table.unveil(Some(b"/a/b"), Some(b"w")).expect("b");
    assert_eq!(table.check(b"/a/b/c", perms(b"w")), Ok(()));
    assert_eq!(table.check(b"/a/b/c", perms(b"r")), Err(EACCES));
}

#[test]
fn locking_rejects_later_calls() {
    let mut table = UnveilTable::new();
    table.unveil(None, None).expect("lock");
    assert_eq!(table.state(), UnveilState::Locked);
    assert_eq!(table.unveil(Some(b"/a"), Some(b"r")), Err(EPERM));
}

#[test]
fn exactly_one_none_is_invalid() {
    let mut table = UnveilTable::new();
    assert_eq!(table.unveil(None, Some(b"r")), Err(EINVAL));
    assert_eq!(table.unveil(Some(b"/a"), None), Err(EINVAL));
}

#[test]
fn a_relative_path_is_invalid() {
    let mut table = UnveilTable::new();
    assert_eq!(table.unveil(Some(b"a"), Some(b"r")), Err(EINVAL));
}

#[test]
fn an_unknown_permission_letter_is_invalid() {
    let mut table = UnveilTable::new();
    assert_eq!(table.unveil(Some(b"/a"), Some(b"rz")), Err(EINVAL));
}

#[test]
fn an_empty_permission_string_is_allowed() {
    let mut table = UnveilTable::new();
    table.unveil(Some(b"/a"), Some(b"")).expect("unveil");
    assert_eq!(table.check(b"/a", UnveilPerms(0)), Ok(()));
    assert_eq!(table.check(b"/a", UnveilPerms(UnveilPerms::R)), Err(EACCES));
}

#[test]
fn a_second_call_replaces_the_permissions() {
    let mut table = UnveilTable::new();
    table.unveil(Some(b"/a"), Some(b"r")).expect("first");
    table.unveil(Some(b"/a"), Some(b"w")).expect("second");
    assert_eq!(table.len(), 1);
    assert_eq!(table.check(b"/a", perms(b"w")), Ok(()));
    assert_eq!(table.check(b"/a", perms(b"r")), Err(EACCES));
}

#[test]
fn the_table_holds_one_hundred_twenty_eight_entries() {
    let mut table = UnveilTable::new();
    for index in 0..MAX_UNVEIL {
        let path = alloc::format!("/p{index}");
        table
            .unveil(Some(path.as_bytes()), Some(b"r"))
            .expect("unveil");
    }
    assert_eq!(table.len(), MAX_UNVEIL);
}

#[test]
fn the_one_hundred_twenty_ninth_entry_is_e2big() {
    let mut table = UnveilTable::new();
    for index in 0..MAX_UNVEIL {
        let path = alloc::format!("/p{index}");
        table
            .unveil(Some(path.as_bytes()), Some(b"r"))
            .expect("unveil");
    }
    assert_eq!(table.unveil(Some(b"/overflow"), Some(b"r")), Err(E2BIG));
}

#[test]
fn clear_returns_to_unset() {
    let mut table = UnveilTable::new();
    table.unveil(Some(b"/a"), Some(b"r")).expect("unveil");
    table.clear();
    assert_eq!(table.state(), UnveilState::Unset);
    assert!(table.is_empty());
    assert_eq!(table.check(b"/other", UnveilPerms(UnveilPerms::R)), Ok(()));
}

#[test]
fn a_lock_is_idempotent_only_once() {
    let mut table = UnveilTable::new();
    table.unveil(None, None).expect("lock");
    assert_eq!(table.unveil(None, None), Err(EPERM));
}

#[test]
fn permissions_combine() {
    let rw = perms(b"rw");
    assert!(rw.contains(UnveilPerms(UnveilPerms::R)));
    assert!(rw.contains(UnveilPerms(UnveilPerms::W)));
    assert!(!rw.contains(UnveilPerms(UnveilPerms::X)));
    assert_eq!(perms(b"r"), UnveilPerms(UnveilPerms::R));
    assert!(
        UnveilPerms(UnveilPerms::R)
            .union(UnveilPerms(UnveilPerms::W))
            .contains(perms(b"rw"))
    );
}
