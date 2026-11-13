// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the atfork registry.

use super::*;
use alloc::boxed::Box;
use core::cell::Cell;

struct TestLock {
    locks: Cell<usize>,
    unlocks: Cell<usize>,
    resets: Cell<usize>,
}

impl TestLock {
    fn leak() -> &'static Self {
        Box::leak(Box::new(Self {
            locks: Cell::new(0),
            unlocks: Cell::new(0),
            resets: Cell::new(0),
        }))
    }
}

impl ForkLock for TestLock {
    fn lock(&self) {
        self.locks.set(self.locks.get() + 1);
    }

    fn unlock(&self) {
        self.unlocks.set(self.unlocks.get() + 1);
    }

    fn reset(&self) {
        self.resets.set(self.resets.get() + 1);
    }
}

fn entry(prepare: usize, parent: usize, child: usize, dso: usize) -> Entry {
    Entry {
        prepare,
        parent,
        child,
        dso,
    }
}

#[test]
fn a_new_registry_is_empty() {
    let registry = Registry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
}

#[test]
fn entries_are_kept_in_order() {
    let mut registry = Registry::new();
    registry.register(1, 2, 3, 4).expect("register");
    registry.register(5, 6, 7, 8).expect("register");
    assert_eq!(registry.len(), 2);
}

#[test]
fn prepare_runs_backwards() {
    let mut registry = Registry::new();
    registry.register(1, 0, 0, 0).expect("register");
    registry.register(2, 0, 0, 0).expect("register");
    registry.register(3, 0, 0, 0).expect("register");
    let lock = TestLock::leak();
    let mut order = Vec::new();
    registry.run_prepare(lock, &mut |entry| order.push(entry.prepare));
    assert_eq!(order, vec![3, 2, 1]);
    assert_eq!(lock.locks.get(), 1);
}

#[test]
fn parent_runs_forwards_and_unlocks() {
    let mut registry = Registry::new();
    registry.register(0, 1, 0, 0).expect("register");
    registry.register(0, 2, 0, 0).expect("register");
    let lock = TestLock::leak();
    let mut order = Vec::new();
    registry.run_parent(lock, &mut |entry| order.push(entry.parent));
    assert_eq!(order, vec![1, 2]);
    assert_eq!(lock.unlocks.get(), 1);
    assert_eq!(lock.resets.get(), 0);
}

#[test]
fn child_runs_forwards_and_resets() {
    let mut registry = Registry::new();
    registry.register(0, 0, 1, 0).expect("register");
    registry.register(0, 0, 2, 0).expect("register");
    let lock = TestLock::leak();
    let mut order = Vec::new();
    registry.run_child(lock, &mut |entry| order.push(entry.child));
    assert_eq!(order, vec![1, 2]);
    assert_eq!(lock.resets.get(), 1);
    assert_eq!(lock.unlocks.get(), 0);
}

#[test]
fn an_empty_registry_never_locks() {
    let registry = Registry::new();
    let lock = TestLock::leak();
    registry.run_prepare(lock, &mut |_| {});
    registry.run_parent(lock, &mut |_| {});
    registry.run_child(lock, &mut |_| {});
    assert_eq!(lock.locks.get(), 0);
    assert_eq!(lock.unlocks.get(), 0);
    assert_eq!(lock.resets.get(), 0);
}

#[test]
fn unregister_removes_only_that_dso() {
    let mut registry = Registry::new();
    registry.register(1, 0, 0, 7).expect("register");
    registry.register(2, 0, 0, 8).expect("register");
    registry.unregister(7);
    assert_eq!(registry.len(), 1);
    let lock = TestLock::leak();
    let mut order = Vec::new();
    registry.run_prepare(lock, &mut |entry| order.push(entry.prepare));
    assert_eq!(order, vec![2]);
}

#[test]
fn a_full_registry_is_enomem() {
    let mut registry = Registry::new();
    for index in 0..MAX_ATFORK {
        registry.register(index, 0, 0, 0).expect("register");
    }
    assert_eq!(registry.register(0, 0, 0, 0), Err(ENOMEM));
}

#[test]
fn handlers_keep_their_addresses_and_dso() {
    let mut registry = Registry::new();
    registry.register(1, 2, 3, 4).expect("register");
    let lock = TestLock::leak();
    let mut seen = Vec::new();
    registry.run_prepare(lock, &mut |entry| seen.push(*entry));
    assert_eq!(seen, vec![entry(1, 2, 3, 4)]);
}
