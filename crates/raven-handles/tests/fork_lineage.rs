// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integration test: forking a handle table shares lineage nodes instead of creating new ones.

use raven_abi::types::Rights;
use raven_handles::{HandleTable, live_lineage_nodes};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Value(u32);

#[test]
fn fork_shares_lineage_nodes_and_keeps_the_count() {
    let mut source = HandleTable::new();
    let first = source.insert(Value(1), Rights::READ).expect("insert");
    let second = source
        .insert(Value(2), Rights::READ.union(Rights::WRITE))
        .expect("insert");
    source.close(first).expect("close");
    let reused = source.insert(Value(3), Rights::MAP).expect("insert");
    let before = live_lineage_nodes();

    let mut destination = HandleTable::new();
    destination
        .fork_into(&source, |value| Ok(value.clone()))
        .expect("fork");
    let after = live_lineage_nodes();
    assert_eq!(after, before, "fork must reuse the source lineage nodes");

    let source_node = source.lineage_node(second).expect("source node");
    let destination_node = destination.lineage_node(second).expect("destination node");
    assert!(std::sync::Arc::ptr_eq(&source_node, &destination_node));
    assert_eq!(destination.len(), 2);
    assert_eq!(destination.get(reused, Rights::MAP), Ok(&Value(3)));
    assert_eq!(destination.get(second, Rights::WRITE), Ok(&Value(2)));
}
