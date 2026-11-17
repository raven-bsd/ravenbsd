// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the turnstile registry.

use super::*;
use alloc::vec;

fn prio(level: u8) -> Priority {
    Priority::new(level)
}

#[test]
fn acquire_and_owner() {
    let mut t = Turnstiles::new();
    assert_eq!(t.acquire(0, 7), Err(Error::UnknownThread));
    t.register(7, prio(10)).unwrap();
    t.acquire(0, 7).unwrap();
    assert_eq!(t.owner(0), Some(7));
    assert!(t.is_owner(0, 7));
    // A second acquire by anyone is rejected.
    assert_eq!(t.acquire(0, 7), Err(Error::AlreadyOwned { owner: 7 }));
    t.register(8, prio(10)).unwrap();
    assert_eq!(t.acquire(0, 8), Err(Error::AlreadyOwned { owner: 7 }));
}

#[test]
fn register_is_rejected_twice() {
    let mut t = Turnstiles::new();
    t.register(1, prio(3)).unwrap();
    assert_eq!(t.register(1, prio(4)), Err(Error::DuplicateThread));
    assert_eq!(t.base_priority(1), Some(prio(3)));
}

#[test]
fn block_orders_waiters_by_priority_then_arrival() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap(); // owner, low
    t.register(1, prio(8)).unwrap();
    t.register(2, prio(4)).unwrap(); // highest waiter
    t.acquire(0, 0).unwrap();
    t.block(0, 1).unwrap();
    t.block(0, 2).unwrap();
    assert_eq!(t.waiters(0), vec![2, 1]);
    assert_eq!(t.effective_priority(0), Some(prio(4)));
}

#[test]
fn block_fifo_within_equal_priority() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    t.register(1, prio(5)).unwrap();
    t.register(2, prio(5)).unwrap();
    t.acquire(0, 0).unwrap();
    t.block(0, 1).unwrap();
    t.block(0, 2).unwrap();
    assert_eq!(t.waiters(0), vec![1, 2]);
}

#[test]
fn block_rejections() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    t.register(1, prio(8)).unwrap();
    t.register(2, prio(6)).unwrap();
    // No owner: cannot block.
    assert_eq!(t.block(0, 1), Err(Error::NotOwned));
    t.acquire(0, 0).unwrap();
    // Owning the object.
    assert_eq!(t.block(0, 0), Err(Error::OwnsObject));
    // Unknown thread.
    assert_eq!(t.block(0, 99), Err(Error::UnknownThread));
    // Waiting twice on the same object.
    t.block(0, 1).unwrap();
    assert_eq!(t.block(0, 1), Err(Error::AlreadyWaiting { object: 0 }));
    // Waiting on a second object.
    t.acquire(1, 2).unwrap();
    assert_eq!(t.block(1, 1), Err(Error::BlockedElsewhere { object: 0 }));
}

#[test]
fn wake_one_hands_off_and_restores() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap(); // owner
    t.register(1, prio(8)).unwrap();
    t.register(2, prio(4)).unwrap(); // highest waiter
    t.acquire(0, 0).unwrap();
    t.block(0, 1).unwrap();
    t.block(0, 2).unwrap();
    assert_eq!(t.effective_priority(0), Some(prio(4)));

    // Highest waiter gets the lock.
    assert_eq!(t.wake_one(0), Some(2));
    assert_eq!(t.owner(0), Some(2));
    // The old owner is restored to its base priority.
    assert_eq!(t.effective_priority(0), Some(prio(10)));
    // The new owner inherits from the remaining waiter.
    assert_eq!(t.effective_priority(2), Some(prio(4)));

    // Next wake hands off to the last waiter.
    assert_eq!(t.wake_one(0), Some(1));
    assert_eq!(t.owner(0), Some(1));
    assert_eq!(t.effective_priority(2), Some(prio(4)));

    // No waiters left: the object is released.
    assert_eq!(t.wake_one(0), None);
    assert_eq!(t.owner(0), None);
}

#[test]
fn wake_all_wakes_each_once_in_order() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    t.register(1, prio(8)).unwrap();
    t.register(2, prio(4)).unwrap();
    t.acquire(0, 0).unwrap();
    t.block(0, 1).unwrap();
    t.block(0, 2).unwrap();
    let woken = t.wake_all(0);
    assert_eq!(woken, vec![2, 1]);
    assert_eq!(t.owner(0), None);
    assert!(t.waiters(0).is_empty());
    // Everyone is restored to base.
    assert_eq!(t.effective_priority(0), Some(prio(10)));
    assert_eq!(t.effective_priority(1), Some(prio(8)));
    assert_eq!(t.effective_priority(2), Some(prio(4)));
    // Nothing left to wake.
    assert_eq!(t.wake_one(0), None);
    assert_eq!(t.wake_all(0), Vec::<ThreadId>::new());
}

#[test]
fn cancel_removes_without_waking() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    t.register(1, prio(8)).unwrap();
    t.register(2, prio(4)).unwrap();
    t.acquire(0, 0).unwrap();
    t.block(0, 1).unwrap();
    t.block(0, 2).unwrap();

    let out = t.cancel(0, 1);
    assert!(out.removed);
    assert_eq!(t.waiters(0), vec![2]);
    assert_eq!(t.effective_priority(0), Some(prio(4)));

    // Cancelling a non-waiter is a no-op and loses nothing.
    let out = t.cancel(0, 1);
    assert!(!out.removed);
    assert_eq!(t.waiters(0), vec![2]);

    assert_eq!(t.wake_one(0), Some(2));
    assert!(t.waiters(0).is_empty());
}

#[test]
fn cancel_restores_owner_when_last_waiter_leaves() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    t.register(1, prio(8)).unwrap();
    t.acquire(0, 0).unwrap();
    t.block(0, 1).unwrap();
    assert_eq!(t.effective_priority(0), Some(prio(8)));
    let out = t.cancel(0, 1);
    assert!(out.removed);
    assert_eq!(t.effective_priority(0), Some(prio(10)));
}

#[test]
fn transitive_inheritance_across_a_chain() {
    // A (high) blocks on B's lock; B blocks on C's lock; C must inherit A's priority.
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap(); // C
    t.register(1, prio(8)).unwrap(); // B
    t.register(2, prio(2)).unwrap(); // A (highest)
    t.acquire(1, 1).unwrap(); // B owns lock 1
    t.acquire(0, 0).unwrap(); // C owns lock 0
    // A blocks on B.
    t.block(1, 2).unwrap();
    assert_eq!(t.effective_priority(1), Some(prio(2)));
    // B blocks on C: C inherits A's priority transitively.
    t.block(0, 1).unwrap();
    assert_eq!(t.effective_priority(0), Some(prio(2)));
    assert_eq!(t.effective_priority(1), Some(prio(2)));
}

#[test]
fn no_priority_inversion_in_the_stated_model() {
    // L (10) holds; H (2) waits; M (8) is merely runnable. While H waits, L runs at H's priority,
    // which is more urgent than M, so M cannot preempt L while L holds the lock.
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap(); // L
    t.register(1, prio(8)).unwrap(); // M (never blocks)
    t.register(2, prio(2)).unwrap(); // H
    t.acquire(0, 0).unwrap();
    t.block(0, 2).unwrap();
    let owner = t.effective_priority(0).unwrap();
    let medium = prio(8);
    assert!(owner < medium);
}

#[test]
fn boost_is_idempotent_and_reports_effect() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    t.register(1, prio(8)).unwrap();
    t.acquire(0, 0).unwrap();
    t.block(0, 1).unwrap();
    // Already boosted by block; a re-boost reports no change.
    let effect = t.boost(0).unwrap().unwrap();
    assert_eq!(effect.owner, 0);
    assert_eq!(effect.before, prio(8));
    assert_eq!(effect.after, prio(8));
    assert!(!effect.changed());
    // Boost needs an owner.
    assert_eq!(t.boost(7), Err(Error::NotOwned));
}

#[test]
fn unboost_requires_no_waiters_and_reports_restoration() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    t.register(1, prio(8)).unwrap();
    t.acquire(0, 0).unwrap();
    t.block(0, 1).unwrap();
    // Cannot unboost while a waiter is present.
    assert_eq!(t.unboost(0), Err(Error::WaitersRemain));
    // An owner without waiters reports its base priority and no change.
    t.register(2, prio(5)).unwrap();
    t.acquire(1, 2).unwrap();
    let effect = t.unboost(1).unwrap().unwrap();
    assert_eq!(effect.owner, 2);
    assert_eq!(effect.before, prio(5));
    assert_eq!(effect.after, prio(5));
    assert!(!effect.changed());
    // Unboost on an unowned object is an error.
    assert_eq!(t.unboost(7), Err(Error::NotOwned));
}

#[test]
fn multi_object_owner_restores_only_when_all_waiters_leave() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap(); // owner of both
    t.register(1, prio(2)).unwrap(); // waits on 0
    t.register(2, prio(4)).unwrap(); // waits on 1
    t.acquire(0, 0).unwrap();
    t.acquire(1, 0).unwrap();
    t.block(0, 1).unwrap();
    t.block(1, 2).unwrap();
    // Highest waiter across both is priority 2.
    assert_eq!(t.effective_priority(0), Some(prio(2)));
    // Release object 0: owner still boosted by object 1's waiter.
    assert_eq!(t.wake_one(0), Some(1));
    assert_eq!(t.effective_priority(0), Some(prio(4)));
    // Release object 1: fully restored.
    assert_eq!(t.wake_one(1), Some(2));
    assert_eq!(t.effective_priority(0), Some(prio(10)));
}

#[test]
fn woken_once_and_never_again() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    t.register(1, prio(8)).unwrap();
    t.acquire(0, 0).unwrap();
    t.block(0, 1).unwrap();
    assert_eq!(t.wake_one(0), Some(1));
    assert!(!t.is_waiting(0, 1));
    assert!(!t.cancel(0, 1).removed);
    assert_eq!(t.wake_one(0), None);
    assert_eq!(t.wake_all(0), Vec::<ThreadId>::new());
}

#[test]
fn seeded_bug_effective_mismatch_is_detected() {
    let mut t = Turnstiles::new();
    t.register(0, prio(0)).unwrap();
    t.acquire(0, 0).unwrap();
    // Corrupt the derived cache.
    t.effective.insert(0, prio(1));
    assert_eq!(
        t.check_invariants(),
        Err(InvariantViolation::EffectiveMismatch)
    );
}

#[test]
fn seeded_bug_waiter_order_is_detected() {
    let mut t = Turnstiles::new();
    t.register(0, prio(5)).unwrap();
    t.register(1, prio(3)).unwrap();
    t.register(2, prio(1)).unwrap();
    t.acquire(0, 0).unwrap();
    t.block(0, 1).unwrap();
    t.block(0, 2).unwrap();
    // Reverse priorities by hand to break the ordering invariant.
    let turnstile = t.objects.get_mut(&0).unwrap();
    turnstile.waiters[0].priority = prio(3);
    turnstile.waiters[1].priority = prio(1);
    assert_eq!(
        t.check_invariants(),
        Err(InvariantViolation::WaiterOrdering)
    );
}
