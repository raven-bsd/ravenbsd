// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn a_lock_starts_unheld() {
    let lock = LockState::new();
    assert!(!lock.is_held());
    assert_eq!(lock.owner(), 0);
}

#[test]
fn the_owner_takes_the_lock() {
    let mut lock = LockState::new();
    assert!(lock.try_acquire(7));
    assert!(lock.is_held());
    assert_eq!(lock.owner(), 7);
}

#[test]
fn a_second_owner_is_refused() {
    let mut lock = LockState::new();
    assert!(lock.try_acquire(1));
    assert!(!lock.try_acquire(2));
    assert_eq!(lock.waits(), 1);
}

#[test]
fn only_the_owner_releases() {
    let mut lock = LockState::new();
    assert!(lock.try_acquire(1));
    assert!(!lock.release(2));
    assert!(lock.release(1));
}

#[test]
fn the_three_locks_are_independent() {
    let mut locks = RtldLock::new();
    assert!(locks.bind.try_acquire(1));
    assert!(locks.phdr.try_acquire(1));
    assert!(!locks.bind.try_acquire(2));
}

#[test]
fn a_released_lock_can_be_retaken() {
    let mut lock = LockState::new();
    assert!(lock.try_acquire(3));
    assert!(lock.release(3));
    assert!(lock.try_acquire(4));
    assert_eq!(lock.owner(), 4);
}
