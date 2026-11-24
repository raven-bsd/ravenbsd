// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded path-manager state tests.

use super::*;

fn v4(last: u8) -> Address {
    Address::V4([192, 0, 2, last])
}

fn path(id: u8) -> Path {
    Path::new(id, v4(id), None)
}

#[test]
fn announce_and_reject_duplicate_ids() {
    let mut manager = PathManager::new();
    manager.announce(path(1)).expect("first");
    assert_eq!(manager.announce(path(1)), Err(Error::Duplicate));
    assert_eq!(manager.len(), 1);
    assert_eq!(manager.get(1).map(|entry| entry.address), Some(v4(1)));
}

#[test]
fn the_table_is_bounded() {
    let mut manager = PathManager::new();
    for id in 0..MAX_PATHS as u8 {
        manager.announce(path(id)).expect("within bound");
    }
    assert_eq!(
        manager.announce(path(MAX_PATHS as u8)),
        Err(Error::PathTableFull)
    );
    assert_eq!(manager.len(), MAX_PATHS);
}

#[test]
fn remove_and_remove_ids_follow_rfc_semantics() {
    let mut manager = PathManager::new();
    manager.announce(path(3)).expect("3");
    manager.announce(path(7)).expect("7");
    manager.announce(path(9)).expect("9");
    assert_eq!(manager.remove(7).expect("remove").address_id, 7);
    assert_eq!(manager.remove(7), Err(Error::NotFound));
    assert_eq!(manager.remove_ids(&[3, 99]), 1);
    assert_eq!(manager.len(), 1);
}

#[test]
fn selection_prefers_regular_paths_and_falls_back_to_backup() {
    let mut manager = PathManager::new();
    manager.announce(path(9)).expect("9");
    manager.announce(path(4)).expect("4");
    manager.set_backup(4, true).expect("backup");
    assert_eq!(manager.select().map(|entry| entry.address_id), Some(9));

    manager.set_backup(9, true).expect("backup");
    assert_eq!(manager.regular_count(), 0);
    assert_eq!(manager.select().map(|entry| entry.address_id), Some(4));

    manager.set_backup(4, false).expect("regular");
    assert_eq!(manager.select().map(|entry| entry.address_id), Some(4));
}

#[test]
fn subflow_handles_are_opaque_and_optional() {
    let mut manager = PathManager::new();
    manager.announce(path(1)).expect("1");
    manager.set_subflow(1, Some(42)).expect("set");
    assert_eq!(manager.get(1).and_then(|entry| entry.subflow), Some(42));
    manager.set_subflow(1, None).expect("clear");
    assert_eq!(manager.get(1).and_then(|entry| entry.subflow), None);
    assert_eq!(manager.set_subflow(2, Some(1)), Err(Error::NotFound));
}
