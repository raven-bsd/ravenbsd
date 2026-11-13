// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! The AVL tree: height bound, the `BTreeMap` model, `twalk` and `tdestroy`.

use std::collections::BTreeSet;

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Node(usize);

struct Arena {
    data: Vec<Entry>,
    free: Vec<usize>,
    freed: usize,
}

impl Arena {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            free: Vec::new(),
            freed: 0,
        }
    }

    fn height(&self, root: Option<Node>) -> usize {
        let Some(root) = root else {
            return 0;
        };
        let left = self.height(self.left(root));
        let right = self.height(self.right(root));
        1 + left.max(right)
    }

    fn inorder(&self, root: Option<Node>, out: &mut Vec<usize>) {
        if let Some(node) = root {
            self.inorder(self.left(node), out);
            out.push(self.key(node));
            self.inorder(self.right(node), out);
        }
    }

    fn check_balances(&self, root: Option<Node>) -> bool {
        self.check_node(root).1
    }

    fn check_node(&self, root: Option<Node>) -> (usize, bool) {
        let Some(node) = root else {
            return (0, true);
        };
        let (left_height, left_ok) = self.check_node(self.left(node));
        let (right_height, right_ok) = self.check_node(self.right(node));
        let expected = left_height as i32 - right_height as i32;
        (
            1 + left_height.max(right_height),
            left_ok && right_ok && expected == i32::from(self.balance(node)),
        )
    }
}

impl NodeStore for Arena {
    type Node = Node;

    fn key(&self, node: Node) -> usize {
        self.data[node.0].key
    }

    fn set_key(&mut self, node: Node, key: usize) {
        self.data[node.0].key = key;
    }

    fn left(&self, node: Node) -> Option<Node> {
        self.data[node.0].left
    }

    fn right(&self, node: Node) -> Option<Node> {
        self.data[node.0].right
    }

    fn balance(&self, node: Node) -> i8 {
        self.data[node.0].balance
    }

    fn set_left(&mut self, node: Node, child: Option<Node>) {
        self.data[node.0].left = child;
    }

    fn set_right(&mut self, node: Node, child: Option<Node>) {
        self.data[node.0].right = child;
    }

    fn set_balance(&mut self, node: Node, balance: i8) {
        self.data[node.0].balance = balance;
    }

    fn alloc(&mut self, key: usize) -> Option<Node> {
        if let Some(index) = self.free.pop() {
            self.data[index] = Entry {
                key,
                left: None,
                right: None,
                balance: 0,
            };
            return Some(Node(index));
        }
        let index = self.data.len();
        self.data.push(Entry {
            key,
            left: None,
            right: None,
            balance: 0,
        });
        Some(Node(index))
    }

    fn free(&mut self, node: Node) {
        self.freed += 1;
        self.free.push(node.0);
    }
}

struct Entry {
    key: usize,
    left: Option<Node>,
    right: Option<Node>,
    balance: i8,
}

fn cmp(a: usize, b: usize) -> i32 {
    match a.cmp(&b) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

#[test]
fn sorted_inserts_keep_the_avl_height_bound() {
    let mut arena = Arena::new();
    let mut root = None;
    let mut cmp_fn = cmp;
    for key in 0..100_000usize {
        tsearch(&mut arena, &mut root, key, &mut cmp_fn).unwrap();
    }
    let height = arena.height(root);
    let bound = (1.44 * ((100_000f64).log2())) as usize + 1;
    assert!(height <= bound, "height {height} > {bound}");
    assert!(arena.check_balances(root));
    let mut keys = Vec::new();
    arena.inorder(root, &mut keys);
    assert_eq!(keys, (0..100_000).collect::<Vec<_>>());
}

#[test]
fn duplicates_return_the_existing_node() {
    let mut arena = Arena::new();
    let mut root = None;
    let mut cmp_fn = cmp;
    let first = tsearch(&mut arena, &mut root, 42, &mut cmp_fn).unwrap();
    let second = tsearch(&mut arena, &mut root, 42, &mut cmp_fn).unwrap();
    assert_eq!(first, second);
    assert_eq!(arena.freed, 0);
}

#[test]
fn two_hundred_thousand_operations_match_a_btree() {
    let mut arena = Arena::new();
    let mut root = None;
    let mut model = BTreeSet::new();
    let mut cmp_fn = cmp;
    let mut state = 0x1234_5678_9abc_def0u64;
    for step in 0..200_000u64 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let key = (state % 20_000) as usize;
        let found = tfind(&arena, root, key, &mut cmp_fn).is_some();
        assert_eq!(found, model.contains(&key), "find step {step}");
        match state % 3 {
            0 => {
                let node = tsearch(&mut arena, &mut root, key, &mut cmp_fn);
                assert!(node.is_some());
                model.insert(key);
            }
            1 => {
                let result = tdelete(&mut arena, &mut root, key, &mut cmp_fn);
                assert_eq!(result.is_some(), model.remove(&key), "delete step {step}");
            }
            _ => {
                let node = tsearch(&mut arena, &mut root, key, &mut cmp_fn);
                assert!(node.is_some());
                model.insert(key);
            }
        }
        if step % 5_000 == 0 {
            let mut keys = Vec::new();
            arena.inorder(root, &mut keys);
            assert_eq!(keys, model.iter().copied().collect::<Vec<_>>());
            assert!(arena.check_balances(root), "balances at step {step}");
        }
    }
    let mut keys = Vec::new();
    arena.inorder(root, &mut keys);
    assert_eq!(keys, model.iter().copied().collect::<Vec<_>>());
}

#[test]
fn tdelete_returns_the_parent_or_the_root_marker() {
    let mut arena = Arena::new();
    let mut root = None;
    let mut cmp_fn = cmp;
    for key in [50usize, 25, 75, 10, 30] {
        tsearch(&mut arena, &mut root, key, &mut cmp_fn).unwrap();
    }
    let result = tdelete(&mut arena, &mut root, 10, &mut cmp_fn);
    assert!(matches!(result, Some(TDelete::Parent(_))));
    assert_eq!(tdelete(&mut arena, &mut root, 999, &mut cmp_fn), None);
    let root_key = arena.key(root.unwrap());
    let result = tdelete(&mut arena, &mut root, root_key, &mut cmp_fn);
    assert_eq!(result, Some(TDelete::RootDeleted));
}

#[test]
fn twalk_visits_a_fixed_tree_in_order() {
    let mut arena = Arena::new();
    let mut root = None;
    let mut cmp_fn = cmp;
    for key in [4usize, 2, 6, 1, 3, 5, 7] {
        tsearch(&mut arena, &mut root, key, &mut cmp_fn).unwrap();
    }
    let mut visits = Vec::new();
    twalk(&arena, root, &mut |node, visit, level| {
        visits.push((arena.key(node), visit, level));
    });
    let expected = vec![
        (4, Visit::Preorder, 0),
        (2, Visit::Preorder, 1),
        (1, Visit::Leaf, 2),
        (2, Visit::Postorder, 1),
        (3, Visit::Leaf, 2),
        (2, Visit::Endorder, 1),
        (4, Visit::Postorder, 0),
        (6, Visit::Preorder, 1),
        (5, Visit::Leaf, 2),
        (6, Visit::Postorder, 1),
        (7, Visit::Leaf, 2),
        (6, Visit::Endorder, 1),
        (4, Visit::Endorder, 0),
    ];
    assert_eq!(visits, expected);
}

#[test]
fn tdestroy_frees_every_node_exactly_once() {
    let mut arena = Arena::new();
    let mut root = None;
    let mut cmp_fn = cmp;
    for key in 0..1000usize {
        tsearch(&mut arena, &mut root, key, &mut cmp_fn).unwrap();
    }
    let nodes = arena.data.len();
    let mut freed_keys = Vec::new();
    tdestroy(&mut arena, root, |key| freed_keys.push(key));
    assert_eq!(arena.freed, nodes);
    assert_eq!(freed_keys.len(), nodes);
    let mut sorted = freed_keys;
    sorted.sort_unstable();
    assert_eq!(sorted, (0..1000).collect::<Vec<_>>());
}

#[test]
fn empty_tree_calls() {
    let mut arena = Arena::new();
    let mut root = None;
    let mut cmp_fn = cmp;
    assert_eq!(tfind(&arena, root, 1, &mut cmp_fn), None);
    assert_eq!(tdelete(&mut arena, &mut root, 1, &mut cmp_fn), None);
    tdestroy(&mut arena, root, |_| panic!("no keys"));
    twalk(&arena, root, &mut |_, _, _| panic!("no visits"));
}
