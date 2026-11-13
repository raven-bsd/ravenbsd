// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the fork-lock registry.

use super::*;
use alloc::boxed::Box;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU8, Ordering};

struct TestLock {
    id: u8,
    log: Arc<AtomicU8>,
}

impl TestLock {
    fn leak(id: u8, log: &Arc<AtomicU8>) -> &'static Self {
        Box::leak(Box::new(Self {
            id,
            log: Arc::clone(log),
        }))
    }
}

impl ForkLock for TestLock {
    fn lock(&self) {
        self.log.fetch_add(self.id, Ordering::SeqCst);
    }

    fn unlock(&self) {
        self.log.fetch_sub(self.id, Ordering::SeqCst);
    }

    fn reset(&self) {
        self.log.store(self.id, Ordering::SeqCst);
    }
}

#[test]
fn an_empty_registry_does_nothing() {
    let mut registry = LockRegistry::new();
    let log = Arc::new(AtomicU8::new(0));
    registry.lock_all();
    registry.unlock_all();
    registry.reset_all();
    assert_eq!(log.load(Ordering::SeqCst), 0);
    assert!(registry.is_empty());
}

#[test]
fn a_single_lock_is_taken_and_released() {
    let log = Arc::new(AtomicU8::new(0));
    let lock = TestLock::leak(1, &log);
    let mut registry = LockRegistry::new();
    registry.register("one", lock).expect("register");
    registry.lock_all();
    assert_eq!(log.load(Ordering::SeqCst), 1);
    registry.unlock_all();
    assert_eq!(log.load(Ordering::SeqCst), 0);
}

#[test]
fn locks_are_taken_in_name_order() {
    let log = Arc::new(AtomicU8::new(0));
    let a = TestLock::leak(1, &log);
    let b = TestLock::leak(2, &log);
    let mut registry = LockRegistry::new();
    registry.register("b", b).expect("register");
    registry.register("a", a).expect("register");
    registry.lock_all();
    assert_eq!(log.load(Ordering::SeqCst), 3);
}

#[test]
fn unlock_is_in_reverse_order() {
    let log = Arc::new(AtomicU8::new(0));
    let a = TestLock::leak(1, &log);
    let b = TestLock::leak(2, &log);
    let mut registry = LockRegistry::new();
    registry.register("a", a).expect("register");
    registry.register("b", b).expect("register");
    registry.lock_all();
    registry.unlock_all();
    assert_eq!(log.load(Ordering::SeqCst), 0);
}

#[test]
fn reset_uses_the_forward_order() {
    let log = Arc::new(AtomicU8::new(0));
    let a = TestLock::leak(1, &log);
    let b = TestLock::leak(2, &log);
    let mut registry = LockRegistry::new();
    registry.register("a", a).expect("register");
    registry.register("b", b).expect("register");
    registry.reset_all();
    assert_eq!(log.load(Ordering::SeqCst), 2);
}

#[test]
fn a_duplicate_name_is_rejected() {
    let log = Arc::new(AtomicU8::new(0));
    let a = TestLock::leak(1, &log);
    let b = TestLock::leak(2, &log);
    let mut registry = LockRegistry::new();
    registry.register("same", a).expect("register");
    assert_eq!(registry.register("same", b), Err(EEXIST));
}

#[test]
fn the_registry_reports_its_size() {
    let log = Arc::new(AtomicU8::new(0));
    let a = TestLock::leak(1, &log);
    let mut registry = LockRegistry::new();
    assert_eq!(registry.len(), 0);
    registry.register("a", a).expect("register");
    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());
}
