// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the handle table and the rights-derivation tree.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct TestObject(u32);

fn table() -> HandleTable<TestObject> {
    HandleTable::new()
}

#[test]
fn insert_and_get() {
    let mut table = table();
    let handle = table.insert(TestObject(7), Rights::READ).expect("insert");
    assert_eq!(handle.index(), 0);
    assert_eq!(handle.generation(), 0);
    assert_eq!(table.get(handle, Rights::READ), Ok(&TestObject(7)));
}

#[test]
fn rights_are_checked() {
    let mut table = table();
    let handle = table.insert(TestObject(1), Rights::READ).expect("insert");
    assert_eq!(table.get(handle, Rights::WRITE), Err(Errno::EACCES));
}

#[test]
fn bad_index_is_ebadf() {
    let table = table();
    assert_eq!(
        table.get(Handle::new(3, 0), Rights::NONE),
        Err(Errno::EBADF)
    );
}

#[test]
fn stale_generation_is_ebadf() {
    let mut table = table();
    let handle = table.insert(TestObject(1), Rights::READ).expect("insert");
    table.close(handle).expect("close");
    assert_eq!(table.get(handle, Rights::NONE), Err(Errno::EBADF));
}

#[test]
fn slot_reuse_increments_generation() {
    let mut table = table();
    let first = table.insert(TestObject(1), Rights::READ).expect("insert");
    table.close(first).expect("close");
    let second = table.insert(TestObject(2), Rights::READ).expect("insert");
    assert_eq!(second.index(), first.index());
    assert_eq!(second.generation(), first.generation().wrapping_add(1));
    assert_eq!(table.get(first, Rights::NONE), Err(Errno::EBADF));
    assert_eq!(table.get(second, Rights::READ), Ok(&TestObject(2)));
}

#[test]
fn generation_retires_the_slot_instead_of_wrapping() {
    let mut table = table();
    let first = table.insert(TestObject(1), Rights::READ).expect("insert");
    let mut current = first;
    for _ in 0..255 {
        table.close(current).expect("close");
        current = table.insert(TestObject(1), Rights::READ).expect("insert");
        assert_eq!(current.index(), first.index());
    }
    assert_eq!(current.generation(), u8::MAX);
    table.close(current).expect("retire");
    assert_eq!(table.get(current, Rights::NONE), Err(Errno::EBADF));
    assert_eq!(table.get(first, Rights::NONE), Err(Errno::EBADF));
    let next = table.insert(TestObject(2), Rights::READ).expect("insert");
    assert_ne!(next.index(), first.index());
    assert_eq!(next.generation(), 0);
    assert_eq!(next.index(), first.index() + 1);
    assert_eq!(table.len(), 1);
}

#[test]
fn retired_slot_stays_unused_across_later_inserts() {
    let mut table = table();
    let first = table.insert(TestObject(1), Rights::READ).expect("insert");
    let mut current = first;
    while current.generation() != u8::MAX {
        table.close(current).expect("close");
        current = table.insert(TestObject(1), Rights::READ).expect("insert");
    }
    table.close(current).expect("retire");
    for value in 0..4u32 {
        let handle = table
            .insert(TestObject(value), Rights::READ)
            .expect("insert");
        assert_eq!(handle.index(), first.index() + 1);
        table.close(handle).expect("close");
    }
    assert_eq!(table.get(first, Rights::NONE), Err(Errno::EBADF));
}

#[test]
fn rights_round_trip_for_every_defined_right() {
    const DEFINED: [Rights; 8] = [
        Rights::READ,
        Rights::WRITE,
        Rights::MAP,
        Rights::EXEC,
        Rights::DUP,
        Rights::TRANSFER,
        Rights::SIGNAL,
        Rights::ADMIN,
    ];
    let mut union = Rights::NONE;
    for right in DEFINED {
        union = union.union(right);
    }
    // Adding a right to `Rights::ALL` without adding it here fails this assertion.
    assert_eq!(union, Rights::ALL);
    for right in DEFINED {
        let mut table = table();
        let handle = table.insert(TestObject(9), right).expect("insert");
        let (value, stored) = table
            .get_with_rights(handle, right)
            .expect("get with the stored right");
        assert_eq!(value, &TestObject(9));
        assert_eq!(stored, right);
        assert_eq!(table.rights(handle), Ok(right));
    }
}

#[test]
fn rights_are_read_without_requesting_extra_rights() {
    let mut table = table();
    let handle = table
        .insert(TestObject(4), Rights::READ.union(Rights::SIGNAL))
        .expect("insert");
    assert_eq!(
        table
            .get_with_rights(handle, Rights::NONE)
            .map(|item| item.1),
        Ok(Rights::READ.union(Rights::SIGNAL))
    );
    assert_eq!(table.get(handle, Rights::ADMIN), Err(Errno::EACCES));
}

#[test]
fn closed_slot_rights_are_cleared() {
    let mut table = table();
    let handle = table.insert(TestObject(1), Rights::ALL).expect("insert");
    table.close(handle).expect("close");
    assert_eq!(table.rights(handle), Err(Errno::EBADF));
}

#[test]
fn dup_intersects_rights() {
    let mut table = table();
    let handle = table
        .insert(
            TestObject(3),
            Rights::READ.union(Rights::WRITE).union(Rights::DUP),
        )
        .expect("insert");
    let dup = table.dup(handle, Rights::READ).expect("dup");
    assert_eq!(table.get(dup, Rights::READ), Ok(&TestObject(3)));
    assert_eq!(table.get(dup, Rights::WRITE), Err(Errno::EACCES));
}

#[test]
fn dup_requires_dup_right() {
    let mut table = table();
    let handle = table.insert(TestObject(3), Rights::READ).expect("insert");
    assert_eq!(table.dup(handle, Rights::READ), Err(Errno::EACCES));
}

#[test]
fn close_twice_is_ebadf() {
    let mut table = table();
    let handle = table.insert(TestObject(3), Rights::ALL).expect("insert");
    assert_eq!(table.close(handle), Ok(()));
    assert_eq!(table.close(handle), Err(Errno::EBADF));
}

#[test]
fn capacity_is_enforced() {
    let mut table = table();
    for index in 0..MAX_HANDLES {
        assert!(
            table.insert(TestObject(index as u32), Rights::READ).is_ok(),
            "insert {index}"
        );
    }
    assert_eq!(
        table.insert(TestObject(0), Rights::READ),
        Err(Errno::EMFILE)
    );
}

#[test]
fn close_reuses_the_first_free_slot() {
    let mut table = table();
    let first = table.insert(TestObject(1), Rights::READ).expect("insert");
    let second = table.insert(TestObject(2), Rights::READ).expect("insert");
    table.close(first).expect("close");
    let third = table.insert(TestObject(3), Rights::READ).expect("insert");
    assert_eq!(third.index(), first.index());
    assert_eq!(second.index(), 1);
}

#[test]
fn get_with_no_rights_still_needs_a_live_slot() {
    let table = table();
    assert_eq!(
        table.get(Handle::new(0, 0), Rights::NONE),
        Err(Errno::EBADF)
    );
}

#[test]
fn close_with_wrong_generation_is_ebadf() {
    let mut table = table();
    let handle = table.insert(TestObject(1), Rights::READ).expect("insert");
    assert_eq!(
        table.close(Handle::new(
            handle.index(),
            handle.generation().wrapping_add(1)
        )),
        Err(Errno::EBADF)
    );
}

#[test]
fn is_empty_and_len_track_slots() {
    let mut table = table();
    assert!(table.is_empty());
    let handle = table.insert(TestObject(1), Rights::READ).expect("insert");
    assert_eq!(table.len(), 1);
    table.close(handle).expect("close");
    assert!(table.is_empty());
}

#[test]
fn duplicate_handle_can_be_closed_independently() {
    let mut table = table();
    let original = table
        .insert(TestObject(1), Rights::READ.union(Rights::DUP))
        .expect("insert");
    let duplicate = table.dup(original, Rights::READ).expect("dup");
    table.close(original).expect("close");
    assert_eq!(table.get(duplicate, Rights::READ), Ok(&TestObject(1)));
}

#[test]
fn max_index_handle_is_out_of_range() {
    let table = table();
    assert_eq!(
        table.get(Handle::new(0x00FF_FFFF, 0), Rights::NONE),
        Err(Errno::EBADF)
    );
}

#[test]
fn revoke_makes_a_held_right_eperm() {
    let mut table = table();
    let handle = table
        .insert(TestObject(1), Rights::READ.union(Rights::WRITE))
        .expect("insert");
    let (node, applied) = table.revoke_target(handle, Rights::WRITE).expect("revoke");
    assert_eq!(applied, Rights::WRITE);
    assert_eq!(table.get(handle, Rights::WRITE), Err(Errno::EPERM));
    assert_eq!(table.get(handle, Rights::READ), Ok(&TestObject(1)));
    assert_eq!(table.rights(handle), Ok(Rights::READ));
    assert_eq!(node.revoked(), Rights::WRITE);
}

#[test]
fn revoke_never_granted_right_stays_eacces() {
    let mut table = table();
    let handle = table.insert(TestObject(1), Rights::READ).expect("insert");
    table
        .revoke_target(handle, Rights::WRITE)
        .expect("revoke absent bit");
    assert_eq!(table.get(handle, Rights::WRITE), Err(Errno::EACCES));
}

#[test]
fn revoke_zero_mask_revokes_every_held_right() {
    let mut table = table();
    let handle = table
        .insert(TestObject(1), Rights::READ.union(Rights::WRITE))
        .expect("insert");
    let (_node, applied) = table.revoke_target(handle, Rights::NONE).expect("revoke");
    assert_eq!(applied, Rights::READ.union(Rights::WRITE));
    assert_eq!(table.get(handle, Rights::READ), Err(Errno::EPERM));
    assert_eq!(table.get(handle, Rights::WRITE), Err(Errno::EPERM));
}

#[test]
fn revoke_is_idempotent_and_stale_is_ebadf() {
    let mut table = table();
    let handle = table
        .insert(TestObject(1), Rights::READ.union(Rights::WRITE))
        .expect("insert");
    table
        .revoke_target(handle, Rights::WRITE)
        .expect("first revoke");
    table
        .revoke_target(handle, Rights::WRITE)
        .expect("second revoke is a no-op");
    assert_eq!(table.get(handle, Rights::READ), Ok(&TestObject(1)));
    table.close(handle).expect("close");
    assert!(matches!(
        table.revoke_target(handle, Rights::WRITE),
        Err(Errno::EBADF)
    ));
}

#[test]
fn child_derived_before_revoke_loses_the_right() {
    let mut table = table();
    let parent = table
        .insert(
            TestObject(1),
            Rights::READ.union(Rights::WRITE).union(Rights::DUP),
        )
        .expect("insert");
    let child = table
        .dup(parent, Rights::READ.union(Rights::WRITE))
        .expect("dup");
    let (parent_node, applied) = table.revoke_target(parent, Rights::WRITE).expect("revoke");
    let child_node = table.lineage_node(child).expect("child node");
    assert!(child_node.is_descendant_of(&parent_node));
    child_node.revoke(applied);
    assert_eq!(table.get(child, Rights::WRITE), Err(Errno::EPERM));
    assert_eq!(table.get(child, Rights::READ), Ok(&TestObject(1)));
}

#[test]
fn child_derived_after_revoke_never_had_the_right() {
    let mut table = table();
    let parent = table
        .insert(
            TestObject(1),
            Rights::READ.union(Rights::WRITE).union(Rights::DUP),
        )
        .expect("insert");
    table.revoke_target(parent, Rights::WRITE).expect("revoke");
    let child = table.dup(parent, Rights::ALL).expect("dup");
    assert_eq!(table.rights(child), Ok(Rights::READ.union(Rights::DUP)));
    assert_eq!(table.get(child, Rights::WRITE), Err(Errno::EACCES));
}

#[test]
fn revoked_handle_stays_open_for_remaining_rights() {
    let mut table = table();
    let handle = table
        .insert(
            TestObject(1),
            Rights::READ.union(Rights::WRITE).union(Rights::DUP),
        )
        .expect("insert");
    table.revoke_target(handle, Rights::WRITE).expect("revoke");
    assert_eq!(table.len(), 1);
    let dup = table.dup(handle, Rights::READ).expect("dup remaining");
    assert_eq!(table.get(dup, Rights::READ), Ok(&TestObject(1)));
}

#[test]
fn cross_table_child_sees_the_parent_revoke() {
    let mut parent_table = table();
    let parent = parent_table
        .insert(
            TestObject(1),
            Rights::READ.union(Rights::WRITE).union(Rights::DUP),
        )
        .expect("insert");
    let parent_node = parent_table.lineage_node(parent).expect("parent node");
    let mut child_table = table();
    let child = child_table
        .insert_derived(
            TestObject(2),
            Rights::READ.union(Rights::WRITE),
            &parent_node,
        )
        .expect("derived insert");
    let child_node = child_table.lineage_node(child).expect("child node");
    assert!(child_node.is_descendant_of(&parent_node));
    let (_node, applied) = parent_table
        .revoke_target(parent, Rights::WRITE)
        .expect("revoke");
    child_node.revoke(applied);
    assert_eq!(child_table.get(child, Rights::WRITE), Err(Errno::EPERM));
    assert_eq!(child_table.get(child, Rights::READ), Ok(&TestObject(2)));
}
