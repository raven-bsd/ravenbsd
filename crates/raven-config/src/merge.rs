// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Recursive merge with replace and removal rules.

use std::collections::BTreeMap;

use crate::Node;

/// Merge `overlay` into `base`.
///
/// Tables merge recursively. Scalars and arrays replace. [`Node::Null`] removes the key; a null
/// at the root leaves an empty table.
#[must_use]
pub fn merge(base: &Node, overlay: &Node) -> Node {
    match (base, overlay) {
        (_, Node::Null) => Node::Null,
        (Node::Table(base), Node::Table(overlay)) => {
            let mut merged: BTreeMap<String, Node> = base.clone();
            for (key, value) in overlay {
                match (merged.get(key), value) {
                    (Some(current), value) => {
                        let result = merge(current, value);
                        if result == Node::Null {
                            merged.remove(key);
                        } else {
                            merged.insert(key.clone(), result);
                        }
                    }
                    (None, Node::Null) => {}
                    (None, value) => {
                        merged.insert(key.clone(), value.clone());
                    }
                }
            }
            Node::Table(merged)
        }
        (_, overlay) => overlay.clone(),
    }
}
