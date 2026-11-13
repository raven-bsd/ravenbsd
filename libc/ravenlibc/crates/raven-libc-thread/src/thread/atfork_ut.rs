// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::sync::atomic::{AtomicUsize, Ordering};

static ORDER: AtomicUsize = AtomicUsize::new(0);
static FIRST: AtomicUsize = AtomicUsize::new(0);
static SECOND: AtomicUsize = AtomicUsize::new(0);

fn first() {
    FIRST.store(ORDER.fetch_add(1, Ordering::AcqRel) + 1, Ordering::Release);
}

fn second() {
    SECOND.store(ORDER.fetch_add(1, Ordering::AcqRel) + 1, Ordering::Release);
}

#[test]
fn the_table_starts_empty() {
    let _serial = crate::test_support::serial();
    clear();
    assert_eq!(count(), 0);
}

#[test]
fn registration_increments_the_count() {
    let _serial = crate::test_support::serial();
    clear();
    assert_eq!(pthread_atfork(Some(first), None, None), Ok(()));
    assert_eq!(count(), 1);
    assert_eq!(pthread_atfork(None, Some(second), None), Ok(()));
    assert_eq!(count(), 2);
    clear();
}

#[test]
fn the_table_is_bounded_at_128() {
    let _serial = crate::test_support::serial();
    clear();
    for _ in 0..ATFORK_MAX {
        assert_eq!(pthread_atfork(None, None, None), Ok(()));
    }
    assert_eq!(pthread_atfork(None, None, None), Err(ENOMEM));
    clear();
}

#[test]
fn prepare_handlers_run_in_reverse_order() {
    let _serial = crate::test_support::serial();
    clear();
    ORDER.store(0, Ordering::Release);
    FIRST.store(0, Ordering::Release);
    SECOND.store(0, Ordering::Release);
    pthread_atfork(Some(first), None, None).unwrap();
    pthread_atfork(Some(second), None, None).unwrap();
    run_prepare();
    // second is registered later, so it runs first.
    assert_eq!(SECOND.load(Ordering::Acquire), 1);
    assert_eq!(FIRST.load(Ordering::Acquire), 2);
    clear();
}

#[test]
fn after_handlers_run_in_registration_order() {
    let _serial = crate::test_support::serial();
    clear();
    ORDER.store(0, Ordering::Release);
    FIRST.store(0, Ordering::Release);
    SECOND.store(0, Ordering::Release);
    pthread_atfork(None, Some(first), None).unwrap();
    pthread_atfork(None, Some(second), None).unwrap();
    run_after(false);
    assert_eq!(FIRST.load(Ordering::Acquire), 1);
    assert_eq!(SECOND.load(Ordering::Acquire), 2);
    clear();
}

#[test]
fn child_handlers_are_separate_from_parent_handlers() {
    let _serial = crate::test_support::serial();
    clear();
    ORDER.store(0, Ordering::Release);
    FIRST.store(0, Ordering::Release);
    SECOND.store(0, Ordering::Release);
    pthread_atfork(None, Some(first), Some(second)).unwrap();
    run_after(true);
    assert_eq!(SECOND.load(Ordering::Acquire), 1);
    assert_eq!(FIRST.load(Ordering::Acquire), 0);
    clear();
}

#[test]
fn the_handler_lock_is_rank_one() {
    assert_eq!(RANK, crate::sync::rank::ATFORK);
    let _serial = crate::test_support::serial();
    clear();
    let kernel = crate::test_support::MockKernel::new();
    lock_handlers(&kernel);
    unlock_handlers(&kernel);
}

#[test]
fn destroy_and_clear_are_stable() {
    let _serial = crate::test_support::serial();
    clear();
    clear();
    assert_eq!(count(), 0);
}
