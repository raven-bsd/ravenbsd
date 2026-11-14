// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn an_empty_table_has_no_entries() {
    assert_eq!(LinkMapTable::new().len(), 0);
}

#[test]
fn add_and_get_round_trip() {
    let mut table = LinkMapTable::new();
    let index = table.add(ObjId(3), 0x1000, 0x2000).expect("add");
    let entry = table.get(index).expect("get");
    assert_eq!(entry.id, 3);
    assert_eq!(entry.l_addr, 0x1000);
    assert_eq!(entry.l_ld, 0x2000);
    assert_eq!(table.len(), 1);
}

#[test]
fn delete_removes_the_entry() {
    let mut table = LinkMapTable::new();
    let index = table.add(ObjId(1), 0, 0).expect("add");
    assert!(table.delete(index));
    assert!(table.get(index).is_none());
    assert_eq!(table.len(), 0);
}

#[test]
fn add_uses_a_free_slot_after_delete() {
    let mut table = LinkMapTable::new();
    let first = table.add(ObjId(1), 0, 0).expect("first");
    assert!(table.delete(first));
    let again = table.add(ObjId(2), 0, 0).expect("second");
    assert_eq!(again, first);
}
