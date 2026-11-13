// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2015 Nuxi, https://nuxi.nl/
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/stdlib/tsearch.c, tdelete.c and tsearch_path.h
// @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tags: $FreeBSD$ absent in the Nuxi files; tsearch.c and tdelete.c 2015)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)] // RavenBSD: AVL balance arithmetic

//! The FreeBSD AVL `<search.h>` tree over an opaque [`NodeStore`].
//!
//! RavenBSD: the donor's two-word bit path (`tsearch_path.h`) becomes an iterative `Vec` of
//! parent links; insertions and deletions still use the donor's balance values (+1 left-heavy,
//! -1 right-heavy) and rotation rules. `tsearch` returns the node, and the shell hands the caller
//! `&node.key` (A-stdlib-13). `tdelete` returns [`TDelete::RootDeleted`] for a root deletion,
//! which is the donor's observed `(void *)1` return.

use alloc::vec::Vec;

/// The arena seam: an implementation of `NodeStore` owns the node memory.
pub trait NodeStore {
    /// An opaque node handle.
    type Node: Copy + Eq;

    /// The caller's opaque key word stored in the node.
    fn key(&self, node: Self::Node) -> usize;
    /// Replace the key word (`tdelete`'s predecessor copy).
    fn set_key(&mut self, node: Self::Node, key: usize);
    /// The left child.
    fn left(&self, node: Self::Node) -> Option<Self::Node>;
    /// The right child.
    fn right(&self, node: Self::Node) -> Option<Self::Node>;
    /// The left-minus-right balance, one of -1, 0, +1 outside a rotation.
    fn balance(&self, node: Self::Node) -> i8;
    /// Set the left child.
    fn set_left(&mut self, node: Self::Node, child: Option<Self::Node>);
    /// Set the right child.
    fn set_right(&mut self, node: Self::Node, child: Option<Self::Node>);
    /// Set the balance.
    fn set_balance(&mut self, node: Self::Node, balance: i8);
    /// Allocate a node for `key`.
    fn alloc(&mut self, key: usize) -> Option<Self::Node>;
    /// Release a node.
    fn free(&mut self, node: Self::Node);
}

/// The result of a successful `tdelete`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum TDelete<N> {
    /// The deleted node was below this parent.
    Parent(N),
    /// The deleted node was the root (the donor returns `(void *)1` here).
    RootDeleted,
}

/// The order in which `twalk` reports a node.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Visit {
    /// Before both children.
    Preorder,
    /// Between the children.
    Postorder,
    /// After both children.
    Endorder,
    /// A leaf (reported once, instead of the three other visits).
    Leaf,
}

/// Find a node by key.
pub fn tfind<S: NodeStore, F: FnMut(usize, usize) -> i32>(
    store: &S,
    root: Option<S::Node>,
    key: usize,
    cmp: &mut F,
) -> Option<S::Node> {
    let mut current = root;
    while let Some(node) = current {
        let result = cmp(key, store.key(node));
        if result == 0 {
            return Some(node);
        }
        current = if result < 0 {
            store.left(node)
        } else {
            store.right(node)
        };
    }
    None
}

/// Insert `key` and return the node holding the key slot; a duplicate returns the existing node.
pub fn tsearch<S: NodeStore, F: FnMut(usize, usize) -> i32>(
    store: &mut S,
    root: &mut Option<S::Node>,
    key: usize,
    cmp: &mut F,
) -> Option<S::Node> {
    let mut path: Vec<(S::Node, bool)> = Vec::new();
    let mut current = *root;
    while let Some(node) = current {
        let result = cmp(key, store.key(node));
        if result == 0 {
            return Some(node);
        }
        if result < 0 {
            path.push((node, true));
            current = store.left(node);
        } else {
            path.push((node, false));
            current = store.right(node);
        }
    }
    let inserted = store.alloc(key)?;
    store.set_balance(inserted, 0);
    store.set_left(inserted, None);
    store.set_right(inserted, None);
    match path.last() {
        None => *root = Some(inserted),
        Some((parent, went_left)) => {
            if *went_left {
                store.set_left(*parent, Some(inserted));
            } else {
                store.set_right(*parent, Some(inserted));
            }
        }
    }
    // Walk back up, adjusting balances and rotating at the first imbalance.
    let mut index = path.len();
    while index > 0 {
        index = index.wrapping_sub(1);
        let Some((node, went_left)) = path.get(index).copied() else {
            break;
        };
        let mut balance = store.balance(node);
        if went_left {
            balance = balance.wrapping_add(1);
        } else {
            balance = balance.wrapping_sub(1);
        }
        store.set_balance(node, balance);
        if balance == 0 {
            break;
        }
        if !(-1..=1).contains(&balance) {
            let new_root = rotate(store, node, balance);
            let parent = if index == 0 {
                None
            } else {
                path.get(index.wrapping_sub(1)).map(|entry| entry.0)
            };
            let parent_left = path
                .get(index.wrapping_sub(1))
                .map(|entry| entry.1)
                .unwrap_or(true);
            match parent {
                None => *root = Some(new_root),
                Some(parent) => {
                    if parent_left {
                        store.set_left(parent, Some(new_root));
                    } else {
                        store.set_right(parent, Some(new_root));
                    }
                }
            }
            break;
        }
    }
    Some(inserted)
}

/// Rotate the subtree rooted at `node` and return the new subtree root.
fn rotate<S: NodeStore>(store: &mut S, node: S::Node, balance: i8) -> S::Node {
    if balance > 1 {
        let Some(left) = store.left(node) else {
            return node;
        };
        if store.balance(left) < 0 {
            // Left-right case.
            let Some(grand) = store.right(left) else {
                return node;
            };
            store.set_right(left, store.left(grand));
            store.set_left(grand, Some(left));
            store.set_left(node, store.right(grand));
            store.set_right(grand, Some(node));
            let grand_balance = store.balance(grand);
            store.set_balance(node, if grand_balance > 0 { -1 } else { 0 });
            store.set_balance(left, if grand_balance < 0 { 1 } else { 0 });
            store.set_balance(grand, 0);
            grand
        } else {
            // Left-left case.
            store.set_left(node, store.right(left));
            store.set_right(left, Some(node));
            store.set_balance(node, 0);
            store.set_balance(left, 0);
            left
        }
    } else {
        let Some(right) = store.right(node) else {
            return node;
        };
        if store.balance(right) > 0 {
            // Right-left case.
            let Some(grand) = store.left(right) else {
                return node;
            };
            store.set_left(right, store.right(grand));
            store.set_right(grand, Some(right));
            store.set_right(node, store.left(grand));
            store.set_left(grand, Some(node));
            let grand_balance = store.balance(grand);
            store.set_balance(node, if grand_balance < 0 { 1 } else { 0 });
            store.set_balance(right, if grand_balance > 0 { -1 } else { 0 });
            store.set_balance(grand, 0);
            grand
        } else {
            // Right-right case.
            store.set_right(node, store.left(right));
            store.set_left(right, Some(node));
            store.set_balance(node, 0);
            store.set_balance(right, 0);
            right
        }
    }
}

/// Delete `key`; `None` when it is absent, otherwise the donor's parent/root result.
pub fn tdelete<S: NodeStore, F: FnMut(usize, usize) -> i32>(
    store: &mut S,
    root: &mut Option<S::Node>,
    key: usize,
    cmp: &mut F,
) -> Option<TDelete<S::Node>> {
    let mut path: Vec<(S::Node, bool)> = Vec::new();
    let mut current = *root;
    let mut parent: Option<S::Node> = None;
    let mut went_left_from_parent = false;
    let mut found = None;
    while let Some(node) = current {
        let result = cmp(key, store.key(node));
        if result == 0 {
            found = Some(node);
            break;
        }
        parent = Some(node);
        if result < 0 {
            path.push((node, true));
            went_left_from_parent = true;
            current = store.left(node);
        } else {
            path.push((node, false));
            went_left_from_parent = false;
            current = store.right(node);
        }
    }
    let node = found?;

    let mut rebalance_path = path.clone();
    if store.left(node).is_none() {
        let replacement = store.right(node);
        link_child(store, root, parent, went_left_from_parent, replacement);
    } else {
        // Replace the node's key by its predecessor's and remove that node instead.
        let mut predecessor_parent = node;
        let mut predecessor_left = true;
        let mut predecessor = store.left(node);
        while let Some(candidate) = predecessor {
            if store.right(candidate).is_none() {
                break;
            }
            predecessor_parent = candidate;
            predecessor_left = false;
            predecessor = store.right(candidate);
        }
        let predecessor = predecessor?;
        let replacement = store.left(predecessor);
        store.set_key(node, store.key(predecessor));
        // The donor continues the same path from the matched node to the predecessor.
        rebalance_path.push((node, true));
        if predecessor_parent != node {
            let mut walker = store.left(node);
            while let Some(candidate) = walker {
                if candidate == predecessor {
                    break;
                }
                rebalance_path.push((candidate, false));
                walker = store.right(candidate);
            }
        }
        link_child(
            store,
            root,
            Some(predecessor_parent),
            predecessor_left,
            replacement,
        );
        store.free(predecessor);
    }
    rebalance_after_delete(store, root, &rebalance_path);
    Some(match parent {
        Some(parent) => TDelete::Parent(parent),
        None => TDelete::RootDeleted,
    })
}

fn link_child<S: NodeStore>(
    store: &mut S,
    root: &mut Option<S::Node>,
    parent: Option<S::Node>,
    went_left: bool,
    child: Option<S::Node>,
) {
    match parent {
        None => *root = child,
        Some(parent) => {
            if went_left {
                store.set_left(parent, child);
            } else {
                store.set_right(parent, child);
            }
        }
    }
}

fn rebalance_after_delete<S: NodeStore>(
    store: &mut S,
    root: &mut Option<S::Node>,
    path: &[(S::Node, bool)],
) {
    let mut index = path.len();
    while index > 0 {
        index = index.wrapping_sub(1);
        let Some((node, went_left)) = path.get(index).copied() else {
            break;
        };
        let mut balance = store.balance(node);
        if went_left {
            balance = balance.wrapping_sub(1);
        } else {
            balance = balance.wrapping_add(1);
        }
        store.set_balance(node, balance);
        if balance == 0 {
            continue;
        }
        if matches!(balance, -1 | 1) {
            break;
        }
        let tall = if balance > 1 {
            store.left(node)
        } else {
            store.right(node)
        };
        let tall_balance = tall.map_or(0, |child| store.balance(child));
        let new_root = rotate_delete(store, node, balance);
        let parent = if index == 0 {
            None
        } else {
            path.get(index.wrapping_sub(1)).map(|entry| entry.0)
        };
        let parent_left = path
            .get(index.wrapping_sub(1))
            .map(|entry| entry.1)
            .unwrap_or(true);
        link_child(store, root, parent, parent_left, Some(new_root));
        if tall_balance == 0 {
            break;
        }
    }
}

/// A deletion rotation: the tall child's balance decides the new balances.
fn rotate_delete<S: NodeStore>(store: &mut S, node: S::Node, balance: i8) -> S::Node {
    if balance > 1 {
        let Some(left) = store.left(node) else {
            return node;
        };
        if store.balance(left) < 0 {
            let Some(grand) = store.right(left) else {
                return node;
            };
            store.set_right(left, store.left(grand));
            store.set_left(grand, Some(left));
            store.set_left(node, store.right(grand));
            store.set_right(grand, Some(node));
            let grand_balance = store.balance(grand);
            store.set_balance(node, if grand_balance > 0 { -1 } else { 0 });
            store.set_balance(left, if grand_balance < 0 { 1 } else { 0 });
            store.set_balance(grand, 0);
            grand
        } else {
            store.set_left(node, store.right(left));
            store.set_right(left, Some(node));
            if store.balance(left) > 0 {
                store.set_balance(node, 0);
                store.set_balance(left, 0);
            } else {
                store.set_balance(node, 1);
                store.set_balance(left, -1);
            }
            left
        }
    } else {
        let Some(right) = store.right(node) else {
            return node;
        };
        if store.balance(right) > 0 {
            let Some(grand) = store.left(right) else {
                return node;
            };
            store.set_left(right, store.right(grand));
            store.set_right(grand, Some(right));
            store.set_right(node, store.left(grand));
            store.set_left(grand, Some(node));
            let grand_balance = store.balance(grand);
            store.set_balance(node, if grand_balance < 0 { 1 } else { 0 });
            store.set_balance(right, if grand_balance > 0 { -1 } else { 0 });
            store.set_balance(grand, 0);
            grand
        } else {
            store.set_right(node, store.left(right));
            store.set_left(right, Some(node));
            if store.balance(right) < 0 {
                store.set_balance(node, 0);
                store.set_balance(right, 0);
            } else {
                store.set_balance(node, -1);
                store.set_balance(right, 1);
            }
            right
        }
    }
}

/// `twalk`: the donor's preorder/postorder/endorder/leaf sequence, level 0 at the root.
pub fn twalk<S: NodeStore, F: FnMut(S::Node, Visit, usize)>(
    store: &S,
    root: Option<S::Node>,
    action: &mut F,
) {
    if let Some(node) = root {
        walk(store, node, 0, action);
    }
}

fn walk<S: NodeStore, F: FnMut(S::Node, Visit, usize)>(
    store: &S,
    node: S::Node,
    level: usize,
    action: &mut F,
) {
    let left = store.left(node);
    let right = store.right(node);
    if left.is_none() && right.is_none() {
        action(node, Visit::Leaf, level);
        return;
    }
    action(node, Visit::Preorder, level);
    if let Some(left) = left {
        walk(store, left, level.wrapping_add(1), action);
    }
    action(node, Visit::Postorder, level);
    if let Some(right) = right {
        walk(store, right, level.wrapping_add(1), action);
    }
    action(node, Visit::Endorder, level);
}

/// The donor's non-recursive `tdestroy`; `free_key` runs once per node.
pub fn tdestroy<S: NodeStore, F: FnMut(usize)>(
    store: &mut S,
    root: Option<S::Node>,
    mut free_key: F,
) {
    #[derive(Clone, Copy)]
    enum Front<N> {
        Back,
        Left(N),
    }

    fn get_front<N: Copy>(
        store: &impl NodeStore<Node = N>,
        back: Option<N>,
        front: Front<N>,
    ) -> Option<N> {
        match front {
            Front::Back => back,
            Front::Left(node) => store.left(node),
        }
    }

    fn set_front<N: Copy>(
        store: &mut impl NodeStore<Node = N>,
        back: &mut Option<N>,
        front: Front<N>,
        value: Option<N>,
    ) {
        match front {
            Front::Back => *back = value,
            Front::Left(node) => store.set_left(node, value),
        }
    }

    let Some(root) = root else {
        return;
    };
    let mut back = Some(root);
    let mut front = Front::Back;
    loop {
        if let Some(current) = get_front(store, back, front) {
            if store.right(current).is_some() {
                front = Front::Left(current);
            } else {
                let left = store.left(current);
                set_front(store, &mut back, front, left);
                free_key(store.key(current));
                store.free(current);
            }
        } else {
            let Some(current) = back else {
                break;
            };
            back = store.left(current);
            if back.is_none() {
                front = Front::Back;
            }
            let right = store.right(current);
            set_front(store, &mut back, front, right);
            free_key(store.key(current));
            store.free(current);
        }
    }
}

#[cfg(test)]
#[path = "avl_ut.rs"]
mod tests;
