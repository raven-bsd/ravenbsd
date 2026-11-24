// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C17: the revocation lineage across a derived handle in another process.

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::types::Rights;
use raven_handles::HandleTable;

use super::{register_lineage, revoke_descendants};

#[test]
fn revoked_mask_is_removed_from_a_derived_handle_in_another_process() {
    // Process A: a root capability and a duplicate (the HANDLE_DUP path, which registers).
    let mut first: HandleTable<u8> = HandleTable::new();
    let root = first.insert(0, Rights::READ | Rights::WRITE).expect("root");
    let derived = first.dup(root, Rights::READ | Rights::WRITE).expect("dup");
    let lineage = first.lineage_node(derived).expect("lineage");
    register_lineage(Arc::clone(&lineage));

    // Process B: a handle derived through the transfer/attachment path, which uses
    // `insert_derived` exactly like handle_xfer/router/duplex/linux do. Registration lets the
    // eager `revoke_descendants` walk mark it; the strong parent chain is what makes the lookup
    // observe the revoke (`LineageNode::revoked`).
    let mut second: HandleTable<u8> = HandleTable::new();
    let child = second
        .insert_derived(1, Rights::READ | Rights::WRITE, &lineage)
        .expect("derived");

    // Revoking on A must reach B (C17 transitivity).
    let (target, applied) = first
        .revoke_target(root, Rights::WRITE)
        .expect("revoke target");

    // The parent chain is the linearization point: B sees the revoke before the eager walk.
    assert_eq!(
        second.get(child, Rights::WRITE).err(),
        Some(Errno::EPERM),
        "C17: the parent chain must expose A's revoke in B"
    );

    revoke_descendants(&target, applied);
    assert_eq!(
        second.get(child, Rights::WRITE).err(),
        Some(Errno::EPERM),
        "C17: HANDLE_REVOKE must remove WRITE from the derived handle in another process"
    );
}
