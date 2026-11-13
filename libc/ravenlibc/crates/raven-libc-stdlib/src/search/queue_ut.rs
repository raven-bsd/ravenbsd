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

//! `insque`/`remque` over a `Vec`-backed list store.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Node(usize);

struct Store {
    next: Vec<Option<Node>>,
    prev: Vec<Option<Node>>,
}

impl Store {
    fn new(n: usize) -> Self {
        Self {
            next: vec![None; n],
            prev: vec![None; n],
        }
    }
    fn forward(&self, mut node: Node, steps: usize) -> Vec<usize> {
        let mut out = vec![node.0];
        for _ in 0..steps {
            match self.next(node) {
                Some(next) => {
                    out.push(next.0);
                    node = next;
                }
                None => break,
            }
        }
        out
    }
}

impl ListStore for Store {
    type Node = Node;

    fn next(&self, node: Node) -> Option<Node> {
        self.next[node.0]
    }

    fn prev(&self, node: Node) -> Option<Node> {
        self.prev[node.0]
    }

    fn set_next(&mut self, node: Node, next: Option<Node>) {
        self.next[node.0] = next;
    }

    fn set_prev(&mut self, node: Node, prev: Option<Node>) {
        self.prev[node.0] = prev;
    }
}

#[test]
fn empty_pred_is_a_one_element_list() {
    let mut store = Store::new(1);
    insque(&mut store, Node(0), None);
    assert_eq!(store.next(Node(0)), None);
    assert_eq!(store.prev(Node(0)), None);
}

#[test]
fn insert_after_and_remove() {
    let mut store = Store::new(4);
    insque(&mut store, Node(0), None);
    insque(&mut store, Node(1), Some(Node(0)));
    insque(&mut store, Node(2), Some(Node(0)));
    assert_eq!(store.forward(Node(0), 2), vec![0, 2, 1]);
    assert_eq!(store.prev(Node(1)), Some(Node(2)));
    remque(&mut store, Node(2));
    assert_eq!(store.forward(Node(0), 2), vec![0, 1]);
    assert_eq!(store.prev(Node(1)), Some(Node(0)));
}

#[test]
fn remove_head_and_tail() {
    let mut store = Store::new(2);
    insque(&mut store, Node(0), None);
    insque(&mut store, Node(1), Some(Node(0)));
    remque(&mut store, Node(1));
    assert_eq!(store.next(Node(0)), None);
    remque(&mut store, Node(0));
    assert_eq!(store.next(Node(0)), None);
    assert_eq!(store.prev(Node(0)), None);
}

#[test]
fn insert_into_the_middle() {
    let mut store = Store::new(3);
    insque(&mut store, Node(0), None);
    insque(&mut store, Node(1), Some(Node(0)));
    insque(&mut store, Node(2), Some(Node(0)));
    assert_eq!(store.forward(Node(0), 2), vec![0, 2, 1]);
    assert_eq!(store.prev(Node(2)), Some(Node(0)));
}

#[test]
fn three_element_order() {
    let mut store = Store::new(3);
    insque(&mut store, Node(0), None);
    insque(&mut store, Node(1), Some(Node(0)));
    insque(&mut store, Node(2), Some(Node(1)));
    assert_eq!(store.forward(Node(0), 2), vec![0, 1, 2]);
}

#[test]
fn remove_middle() {
    let mut store = Store::new(3);
    insque(&mut store, Node(0), None);
    insque(&mut store, Node(1), Some(Node(0)));
    insque(&mut store, Node(2), Some(Node(1)));
    remque(&mut store, Node(1));
    assert_eq!(store.forward(Node(0), 2), vec![0, 2]);
    assert_eq!(store.prev(Node(2)), Some(Node(0)));
}

#[test]
fn remove_head() {
    let mut store = Store::new(2);
    insque(&mut store, Node(0), None);
    insque(&mut store, Node(1), Some(Node(0)));
    remque(&mut store, Node(0));
    assert_eq!(store.prev(Node(1)), None);
}

#[test]
fn single_link_disappears() {
    let mut store = Store::new(1);
    insque(&mut store, Node(0), None);
    remque(&mut store, Node(0));
    assert_eq!(store.next(Node(0)), None);
}
