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
fn direct_handoff_stress_never_loses_a_waiter() {
    // A donor-style direct handoff loop: each round hands the object to every waiter in turn.
    // A lost wake, a stale owner, or an unrestored boost shows up immediately.
    let mut t = Turnstiles::new();
    for thread in 0..8u32 {
        t.register(thread, prio(4 + (thread % 4) as u8)).unwrap();
    }
    for round in 0..200u32 {
        let object = round;
        t.acquire(object, 0).unwrap();
        for waiter in 1..8u32 {
            t.block(object, waiter).unwrap();
        }
        let mut woken = Vec::new();
        while let Some(thread) = t.wake_one(object) {
            woken.push(thread);
        }
        assert_eq!(woken.len(), 7, "round {round}: waiter lost");
        for waiter in 1..8u32 {
            assert!(
                woken.contains(&waiter),
                "round {round}: waiter {waiter} not woken"
            );
        }
        assert_eq!(t.owner(object), None, "round {round}: stale owner");
        assert_eq!(t.waiter_count(object), 0);
        for thread in 0..8u32 {
            assert_eq!(
                t.effective_priority(thread),
                t.base_priority(thread),
                "round {round}: thread {thread} boost not restored"
            );
        }
    }
}

#[test]
fn canceled_handoff_round_restores_owner_and_never_wakes_the_canceled_thread() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    for thread in 1..6 {
        t.register(thread, prio(4)).unwrap();
    }
    for round in 0..200u32 {
        let object = 1000 + round;
        t.acquire(object, 0).unwrap();
        for waiter in 1..6 {
            t.block(object, waiter).unwrap();
        }
        // The middle waiter is canceled before the handoff; the others must still make progress.
        let canceled = 3;
        let outcome = t.cancel(object, canceled);
        assert!(outcome.removed);
        let mut woken = Vec::new();
        while let Some(thread) = t.wake_one(object) {
            assert_ne!(thread, canceled);
            woken.push(thread);
        }
        assert_eq!(
            woken.len(),
            4,
            "round {round}: canceled round lost a waiter"
        );
        assert_eq!(t.owner(object), None);
        assert_eq!(t.effective_priority(0), Some(prio(10)));
    }
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

#[cfg(feature = "lockstat")]
#[test]
fn lockstat_default_zeros_and_timeouts() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    // An untouched object has all-zero statistics.
    assert_eq!(t.stats(7), LockStats::default());
    t.acquire(0, 0).unwrap();
    t.register(1, prio(8)).unwrap();
    t.block(0, 1).unwrap();
    let _ = t.wake_all(0);
    // `timeouts` is reserved for the kernel timeout path and stays zero.
    assert_eq!(t.stats(0).timeouts, 0);
}

#[cfg(feature = "lockstat")]
#[test]
fn lockstat_counters_track_operations() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    t.register(1, prio(8)).unwrap();
    t.register(2, prio(6)).unwrap();

    // Immediate acquisition.
    t.acquire(0, 0).unwrap();
    let s = t.stats(0);
    assert_eq!(s.immediate_acquisitions, 1);
    assert_eq!(s.wait_entries, 0);
    assert_eq!(s.wakes, 0);

    // First block: one wait entry, queue depth 1, owner boosted (10 -> 8).
    t.block(0, 1).unwrap();
    let s = t.stats(0);
    assert_eq!(s.wait_entries, 1);
    assert_eq!(s.max_queue_depth, 1);
    assert_eq!(s.inherited_priority_changes, 1);

    // Second block: queue depth 2, owner boosted again (8 -> 6).
    t.block(0, 2).unwrap();
    let s = t.stats(0);
    assert_eq!(s.wait_entries, 2);
    assert_eq!(s.max_queue_depth, 2);
    assert_eq!(s.inherited_priority_changes, 2);

    // wake_all wakes both waiters.
    assert_eq!(t.wake_all(0).len(), 2);
    let s = t.stats(0);
    assert_eq!(s.wakes, 2);
    assert_eq!(s.timeouts, 0);
}

#[cfg(feature = "lockstat")]
#[test]
fn lockstat_cancel_and_wake_one_counts() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    t.register(1, prio(8)).unwrap();
    t.acquire(0, 0).unwrap();
    t.block(0, 1).unwrap();
    // Cancel removes one waiter and restores the owner (a priority change).
    assert!(t.cancel(0, 1).removed);
    let s = t.stats(0);
    assert_eq!(s.cancels, 1);
    assert_eq!(s.wakes, 0);
    assert_eq!(s.inherited_priority_changes, 2); // block boost + cancel restore

    // wake_one wakes exactly one waiter.
    t.block(0, 1).unwrap();
    assert!(t.wake_one(0).is_some());
    let s = t.stats(0);
    assert_eq!(s.wakes, 1);
    assert_eq!(s.cancels, 1);
}

#[cfg(feature = "lockstat")]
#[test]
fn lockstat_survives_turnstile_reclamation() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    t.register(1, prio(2)).unwrap();
    t.acquire(2, 0).unwrap();
    t.block(2, 1).unwrap();
    assert_eq!(t.wake_one(2), Some(1)); // handoff to the waiter
    assert_eq!(t.wake_one(2), None); // last owner releases: idle turnstile reclaimed
    assert_eq!(t.objects().count(), 0, "empty turnstile must be reclaimed");
    // The counters belong to the lock object, not the transient turnstile: FreeBSD
    // `lock_profile_object` is keyed by `lock_object` and NetBSD `lockdebug` is allocated at
    // lock init and freed at lock destroy. Reclaiming the empty turnstile must not zero them.
    let s = t.stats(2);
    assert_eq!(s.immediate_acquisitions, 1);
    assert_eq!(s.wait_entries, 1);
    assert_eq!(s.wakes, 1);
    let stats_objects: vec::Vec<ObjectId> = t.stats_objects().collect();
    assert_eq!(stats_objects, vec![2]);
}

#[cfg(feature = "lockstat")]
#[test]
fn lockstat_cache_is_bounded() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap();
    for object in 0..MAX_STATS_OBJECTS as ObjectId {
        t.acquire(object, 0).unwrap();
        assert_eq!(t.wake_one(object), None);
    }
    assert_eq!(t.stats_objects().count(), MAX_STATS_OBJECTS);
    // A full cache rejects only the new object; the existing records stay valid.
    let extra = MAX_STATS_OBJECTS as ObjectId;
    t.acquire(extra, 0).unwrap();
    assert_eq!(t.wake_one(extra), None);
    assert_eq!(t.stats(extra), LockStats::default());
    assert_eq!(t.stats_objects().count(), MAX_STATS_OBJECTS);
    assert_eq!(t.stats(0).immediate_acquisitions, 1);
}

#[cfg(feature = "lockstat")]
#[test]
fn lockstat_max_inheritance_depth_is_transitive() {
    let mut t = Turnstiles::new();
    t.register(0, prio(10)).unwrap(); // C
    t.register(1, prio(8)).unwrap(); // B
    t.register(2, prio(2)).unwrap(); // A
    t.acquire(1, 1).unwrap(); // B owns lock 1
    t.acquire(0, 0).unwrap(); // C owns lock 0
    // Bottom-up so the transitive chain A -> B -> C is observable on lock 1.
    t.block(0, 1).unwrap(); // B blocks on C (depth 1 on lock 0)
    t.block(1, 2).unwrap(); // A blocks on B; B already waits on C, so depth 2 on lock 1
    let lock0 = t.stats(0);
    let lock1 = t.stats(1);
    assert_eq!(lock0.max_inheritance_depth, 1);
    assert_eq!(lock1.max_inheritance_depth, 2);
}

#[test]
fn deep_inheritance_chain_32_owners() {
    // 33 threads: thread 0 (highest) blocks on object 1, thread i blocks on object i+1, down to
    // the deepest owner (thread 32). The chain is 32 owners deep.
    const N: usize = 33;
    let mut t = Turnstiles::new();
    t.register(0, prio(0)).unwrap(); // highest priority
    for i in 1..N {
        t.register(i as ThreadId, prio(10 + i as u8)).unwrap();
    }
    // Thread i owns object i.
    for i in 0..N {
        t.acquire(i as ObjectId, i as ThreadId).unwrap();
    }
    // Thread i blocks on object i+1 (32 blocks).
    for i in 0..(N - 1) {
        t.block((i + 1) as ObjectId, i as ThreadId).unwrap();
    }
    // The deepest owner inherits thread 0's highest priority transitively.
    assert_eq!(t.effective_priority((N - 1) as ThreadId), Some(prio(0)));
    // Every intermediate owner is also boosted to the same priority.
    for i in 1..N {
        assert_eq!(t.effective_priority(i as ThreadId), Some(prio(0)));
    }
    t.check_invariants().expect("deep chain invariant");
}

#[test]
fn fairness_equal_priorities_hand_off_in_arrival_order() {
    const N: usize = 64;
    let mut t = Turnstiles::new();
    t.register(0, prio(5)).unwrap(); // owner
    for i in 1..=N {
        t.register(i as ThreadId, prio(3)).unwrap(); // all equal priority
    }
    t.acquire(0, 0).unwrap();
    // N equal-priority waiters block in arrival order.
    for i in 1..=N {
        t.block(0, i as ThreadId).unwrap();
    }
    assert_eq!(t.waiter_count(0), N);
    // wake_one hands the lock off in arrival (FIFO) order, not reverse order.
    for i in 1..=N {
        assert_eq!(t.wake_one(0), Some(i as ThreadId), "handoff {i}");
    }
    assert_eq!(t.wake_one(0), None);
    assert_eq!(t.waiter_count(0), 0);
    assert_eq!(t.owner(0), None);
}

#[test]
fn empty_turnstiles_are_reclaimed() {
    let mut t = Turnstiles::new();
    t.register(1, prio(10)).unwrap();
    t.register(2, prio(20)).unwrap();
    for object in 0..1000u32 {
        t.acquire(object, 1).unwrap();
        t.block(object, 2).unwrap();
        assert_eq!(t.wake_one(object), Some(2));
        assert_eq!(t.wake_one(object), None);
    }
    assert_eq!(t.objects().count(), 0, "empty turnstiles retained");
}
