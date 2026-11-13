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

//! `ThreadAtexit`: LIFO and the DSO reference calls.

use super::*;

#[derive(Default)]
struct Refs {
    added: Vec<usize>,
    dropped: Vec<usize>,
}

impl DsoRefs for Refs {
    fn add_ref(&mut self, dso: usize) {
        self.added.push(dso);
    }
    fn drop_ref(&mut self, dso: usize) {
        self.dropped.push(dso);
    }
}

fn entry(func: usize, dso: usize) -> Entry {
    Entry {
        func,
        arg: 0,
        dso,
        kind: super::super::atexit::Kind::Cxa,
    }
}

#[test]
fn lifo_and_reference_accounting() {
    let mut list = ThreadAtexit::new();
    let mut refs = Refs::default();
    list.push(entry(1, 10), &mut refs).unwrap();
    list.push(entry(2, 20), &mut refs).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(refs.added, vec![10, 20]);
    assert_eq!(list.pop(&mut refs).map(|entry| entry.func), Some(2));
    assert_eq!(list.pop(&mut refs).map(|entry| entry.func), Some(1));
    assert_eq!(list.pop(&mut refs), None);
    assert_eq!(refs.dropped, vec![20, 10]);
    assert!(list.is_empty());
}

#[test]
fn empty_list_and_default() {
    let mut refs = Refs::default();
    let mut list = ThreadAtexit::new();
    assert_eq!(list.pop(&mut refs), None);
    assert_eq!(ThreadAtexit::default(), list);
    assert_eq!(refs.added, Vec::<usize>::new());
}

#[test]
fn three_pushes_pop_in_reverse() {
    let mut list = ThreadAtexit::new();
    let mut refs = Refs::default();
    for func in 1..=3 {
        list.push(entry(func, func), &mut refs).unwrap();
    }
    assert_eq!(list.pop(&mut refs).map(|entry| entry.func), Some(3));
    assert_eq!(list.pop(&mut refs).map(|entry| entry.func), Some(2));
    assert_eq!(list.pop(&mut refs).map(|entry| entry.func), Some(1));
    assert_eq!(refs.dropped, vec![3, 2, 1]);
}

#[test]
fn default_is_empty() {
    assert_eq!(ThreadAtexit::default(), ThreadAtexit::new());
}
