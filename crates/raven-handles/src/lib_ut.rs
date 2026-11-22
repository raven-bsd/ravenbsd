// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the handle table and the rights-derivation tree.

// The test harness links `std`; the library itself stays `no_std`. The K528 race tests need host
// threads and their barriers to force the exact cross-table interleaving.
extern crate std;

use super::*;

use alloc::vec;

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

#[test]
fn for_each_live_skips_holes_and_retired_slots() {
    let mut table = table();
    let first = table.insert(TestObject(1), Rights::READ).expect("insert");
    let second = table.insert(TestObject(2), Rights::WRITE).expect("insert");
    table.close(first).expect("close");
    let reused = table
        .insert(TestObject(3), Rights::MAP)
        .expect("reuse hole");
    assert_eq!(reused.index(), first.index());
    assert_eq!(reused.generation(), first.generation().wrapping_add(1));

    let mut seen = Vec::new();
    table.for_each_live(|handle, value, rights, lineage| {
        let expected = table.lineage_node(handle).expect("lineage");
        assert!(Arc::ptr_eq(lineage, &expected));
        seen.push((handle, value.0, rights));
    });
    assert_eq!(
        seen,
        vec![(reused, 3, Rights::MAP), (second, 2, Rights::WRITE),]
    );
}

#[test]
fn fork_into_preserves_numbers_rights_and_lineage() {
    let mut source = table();
    let first = source.insert(TestObject(1), Rights::READ).expect("insert");
    let second = source.insert(TestObject(2), Rights::WRITE).expect("insert");
    source.close(first).expect("close");
    let reused = source
        .insert(TestObject(3), Rights::MAP)
        .expect("reuse hole");
    let source_node = source.lineage_node(second).expect("node");

    let mut destination = table();
    destination
        .fork_into(&source, |value| Ok(value.clone()))
        .expect("fork");
    assert_eq!(destination.len(), 2);
    assert_eq!(destination.get(reused, Rights::MAP), Ok(&TestObject(3)));
    assert_eq!(destination.get(second, Rights::WRITE), Ok(&TestObject(2)));
    let destination_node = destination.lineage_node(second).expect("node");
    assert!(Arc::ptr_eq(&source_node, &destination_node));
    assert_eq!(
        destination.get(Handle::new(first.index(), first.generation()), Rights::NONE),
        Err(Errno::EBADF)
    );
    let next = destination
        .insert(TestObject(4), Rights::READ)
        .expect("insert after fork");
    assert_eq!(next.index(), 2);
    assert_eq!(next.generation(), 0);
}

#[test]
fn fork_into_preserves_retired_slots() {
    let mut source = table();
    let first = source.insert(TestObject(1), Rights::READ).expect("insert");
    let mut current = first;
    for _ in 0..255 {
        source.close(current).expect("close");
        current = source.insert(TestObject(1), Rights::READ).expect("insert");
    }
    source.close(current).expect("retire");
    let live = source.insert(TestObject(2), Rights::READ).expect("insert");

    let mut destination = table();
    destination
        .fork_into(&source, |value| Ok(value.clone()))
        .expect("fork");
    let next = destination
        .insert(TestObject(3), Rights::READ)
        .expect("insert after retired");
    assert_eq!(next.index(), live.index().saturating_add(1));
}

#[test]
fn fork_into_clone_failure_leaves_destination_empty() {
    let mut source = table();
    source.insert(TestObject(1), Rights::READ).expect("insert");
    source.insert(TestObject(2), Rights::READ).expect("insert");
    let mut destination = table();
    let mut calls = 0u32;
    let result = destination.fork_into(&source, |value| {
        calls = calls.saturating_add(1);
        if value.0 == 2 {
            Err(Errno::ENOMEM)
        } else {
            Ok(value.clone())
        }
    });
    assert_eq!(result, Err(Errno::ENOMEM));
    assert!(destination.is_empty());
    assert_eq!(calls, 2);
}

// K528: a revoke that completed before a derived handle was registered must still bind that
// handle. The kernel's descendant registry is a weak snapshot: a transfer (or an SCM_RIGHTS
// attachment) inserts the child, releases the table lock, and registers it afterwards. If the
// revoke walk snapshots between those steps, the child's own `revoked` mask stays zero. The
// ancestor chain is the linearization point, not the registry.

#[test]
fn completed_ancestor_revoke_binds_a_child_installed_before_registration() {
    let mut parent_table = table();
    let parent = parent_table
        .insert(
            TestObject(1),
            Rights::READ.union(Rights::WRITE).union(Rights::DUP),
        )
        .expect("parent");
    let parent_node = parent_table.lineage_node(parent).expect("parent node");
    let mut child_table = table();
    // T1: the derived child exists in the destination table, but its registration/publication
    // has not happened yet.
    let child = child_table
        .insert_derived(
            TestObject(2),
            Rights::READ.union(Rights::WRITE),
            &parent_node,
        )
        .expect("derived insert");
    // T2: the revoke completes and its registry snapshot cannot see the unregistered child.
    parent_table
        .revoke_target(parent, Rights::WRITE)
        .expect("revoke");
    // T1 publishes the child. The use must still see the completed ancestor revoke: EPERM.
    assert_eq!(
        child_table.get(child, Rights::WRITE),
        Err(Errno::EPERM),
        "a completed ancestor revoke must bind the late-registered child"
    );
    assert_eq!(child_table.get(child, Rights::READ), Ok(&TestObject(2)));
    assert_eq!(child_table.rights(child), Ok(Rights::READ));
}

#[test]
fn barrier_controls_install_revoke_publish_then_use() {
    use std::sync::{Arc as StdArc, Barrier, Mutex};
    use std::thread;

    let mut parents = table();
    let parent = parents
        .insert(
            TestObject(1),
            Rights::READ.union(Rights::WRITE).union(Rights::DUP),
        )
        .expect("parent");
    let parent_node = parents.lineage_node(parent).expect("parent node");
    let parents = StdArc::new(Mutex::new(parents));
    let children = StdArc::new(Mutex::new(table()));
    // Two rendezvous points force the order: install -> revoke complete -> publish -> use.
    let installed = StdArc::new(Barrier::new(2));
    let revoked = StdArc::new(Barrier::new(2));
    let published = StdArc::new(Mutex::new(None));

    let installer = {
        let children = StdArc::clone(&children);
        let installed = StdArc::clone(&installed);
        let revoked = StdArc::clone(&revoked);
        let published = StdArc::clone(&published);
        let parent_node = alloc::sync::Arc::clone(&parent_node);
        thread::spawn(move || {
            let child = children
                .lock()
                .expect("child table")
                .insert_derived(
                    TestObject(2),
                    Rights::READ.union(Rights::WRITE),
                    &parent_node,
                )
                .expect("derived insert");
            installed.wait();
            revoked.wait();
            // T1 registers the child only now, after T2's revoke returned.
            *published.lock().expect("published") = Some(child);
        })
    };

    let revoker = {
        let parents = StdArc::clone(&parents);
        let installed = StdArc::clone(&installed);
        let revoked = StdArc::clone(&revoked);
        thread::spawn(move || {
            installed.wait();
            let mut table = parents.lock().expect("parent table");
            table.revoke_target(parent, Rights::WRITE).expect("revoke");
            // The descendant walk's snapshot predates the child's registration, so it cannot
            // include it. That is the race the K528 contract forbids.
            drop(table);
            revoked.wait();
        })
    };

    installer.join().expect("installer");
    revoker.join().expect("revoker");

    let child = published.lock().expect("published").expect("child handle");
    assert_eq!(
        children
            .lock()
            .expect("child table")
            .get(child, Rights::WRITE),
        Err(Errno::EPERM),
        "install-before-register must not outlive a completed ancestor revoke"
    );
}

#[test]
fn closed_intermediate_handle_keeps_the_ancestor_binding() {
    let mut table = table();
    let parent = table
        .insert(
            TestObject(1),
            Rights::READ.union(Rights::WRITE).union(Rights::DUP),
        )
        .expect("parent");
    let intermediate = table
        .dup(parent, Rights::READ.union(Rights::WRITE).union(Rights::DUP))
        .expect("intermediate");
    let child = table
        .dup(intermediate, Rights::READ.union(Rights::WRITE))
        .expect("child");
    // The strong parent link keeps the chain alive after the intermediate slot closes.
    table.close(intermediate).expect("close intermediate");
    table
        .revoke_target(parent, Rights::WRITE)
        .expect("revoke parent");
    assert_eq!(table.get(child, Rights::WRITE), Err(Errno::EPERM));
    assert_eq!(table.get(child, Rights::READ), Ok(&TestObject(1)));
}

#[test]
fn ancestor_revoke_split_keeps_never_granted_eacces() {
    let mut table = table();
    let parent = table
        .insert(TestObject(1), Rights::READ.union(Rights::DUP))
        .expect("parent");
    // The child asks for MAP as well, but MAP was never granted, so it is not stored.
    let child = table
        .dup(parent, Rights::READ.union(Rights::MAP))
        .expect("child");
    assert_eq!(table.rights(child), Ok(Rights::READ));
    table
        .revoke_target(parent, Rights::READ)
        .expect("revoke read");
    // READ was granted to the child and then revoked on the ancestor: EPERM.
    assert_eq!(table.get(child, Rights::READ), Err(Errno::EPERM));
    // MAP was never granted to the child: stay EACCES, not EPERM.
    assert_eq!(table.get(child, Rights::MAP), Err(Errno::EACCES));
}

#[test]
fn queued_attachment_marked_after_install_is_eperm_not_eacces() {
    let mut parent_table = table();
    let parent = parent_table
        .insert(TestObject(1), Rights::READ.union(Rights::WRITE))
        .expect("parent");
    let parent_node = parent_table.lineage_node(parent).expect("parent node");
    // The right was granted when the attachment was queued, but the revoke completes before
    // install, so `insert_derived` stores only the effective mask.
    parent_table
        .revoke_target(parent, Rights::WRITE)
        .expect("revoke");
    let mut child_table = table();
    let child = child_table
        .insert_derived(
            TestObject(2),
            Rights::READ.union(Rights::WRITE),
            &parent_node,
        )
        .expect("derived insert");
    assert_eq!(child_table.rights(child), Ok(Rights::READ));
    // The kernel install path copies the parent's revoked mask onto the child so the two cases
    // stay distinct; the use is EPERM, not EACCES.
    let child_node = child_table.lineage_node(child).expect("child node");
    child_node.revoke(parent_node.revoked());
    assert_eq!(child_table.get(child, Rights::WRITE), Err(Errno::EPERM));
}

#[test]
fn deep_lineage_sees_a_root_revoke() {
    let mut table = table();
    let root = table
        .insert(
            TestObject(1),
            Rights::READ
                .union(Rights::WRITE)
                .union(Rights::DUP)
                .union(Rights::TRANSFER),
        )
        .expect("root");
    let mut current = root;
    for _ in 0..5 {
        current = table
            .dup(
                current,
                Rights::READ.union(Rights::WRITE).union(Rights::DUP),
            )
            .expect("derive");
    }
    table
        .revoke_target(root, Rights::WRITE)
        .expect("revoke root");
    assert_eq!(table.get(current, Rights::WRITE), Err(Errno::EPERM));
    assert_eq!(table.get(current, Rights::READ), Ok(&TestObject(1)));
}
