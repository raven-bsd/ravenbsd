// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::limits::{EBUSY, ENOTSUP};

#[test]
fn size_and_static_initialiser() {
    assert_eq!(core::mem::size_of::<SpinLock>(), 4);
    let lock = SpinLock::new();
    assert_eq!(lock.word.load(Ordering::Relaxed), 0);
    assert!(!is_locked(&lock));
}

#[test]
fn lock_and_trylock_on_a_free_lock() {
    let lock = SpinLock::new();
    assert_eq!(trylock(&lock), Ok(()));
    assert!(is_locked(&lock));
    unlock(&lock);
    assert!(!is_locked(&lock));
}

#[test]
fn trylock_on_a_held_lock_is_ebusy() {
    let lock = SpinLock::new();
    assert_eq!(trylock(&lock), Ok(()));
    assert_eq!(trylock(&lock), Err(EBUSY));
}

#[test]
fn the_lock_word_is_ebusy_while_held() {
    let lock = SpinLock::new();
    assert_eq!(trylock(&lock), Ok(()));
    assert_eq!(lock.word.load(Ordering::Relaxed), EBUSY.0 as u32);
}

#[test]
fn init_private_resets_the_word() {
    let mut lock = SpinLock {
        word: core::sync::atomic::AtomicU32::new(7),
    };
    assert_eq!(init(&mut lock, 0), Ok(()));
    assert_eq!(lock.word.load(Ordering::Relaxed), 0);
}

#[test]
fn init_shared_is_enotsup() {
    let mut lock = SpinLock::new();
    assert_eq!(init(&mut lock, 1), Err(ENOTSUP));
}

#[test]
fn destroy_keeps_the_value() {
    let lock = SpinLock::new();
    destroy(&lock);
    assert_eq!(lock.word.load(Ordering::Relaxed), 0);
}

#[test]
fn a_locked_lock_can_be_unlocked_from_another_context() {
    let spin = SpinLock::new();
    lock(&spin);
    assert!(is_locked(&spin));
    unlock(&spin);
    assert!(!is_locked(&spin));
}
