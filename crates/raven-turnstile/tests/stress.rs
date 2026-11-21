// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Turnstile stress scenarios for `scripts/lock-stress.sh`.
//!
//! Each test writes `label nanos` samples to `LOCK_STRESS_SAMPLES` when the
//! harness sets it. The model has no sleeping locks, so the measured latency is
//! the operation cost plus host scheduling under the harness load.

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use std::time::Instant;

use raven_turnstile::{Priority, Turnstiles};

const ROUNDS: usize = 256;
static SAMPLE_LOCK: Mutex<()> = Mutex::new(());

fn record(label: &str, nanos: u128) {
    let Ok(path) = std::env::var("LOCK_STRESS_SAMPLES") else {
        return;
    };
    let _guard = SAMPLE_LOCK.lock().expect("sample lock");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{label}\t{nanos}");
    }
}

fn timed<T>(label: &str, work: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let result = work();
    record(label, start.elapsed().as_nanos());
    result
}

#[test]
fn stress_uncontended_acquire_release() {
    let mut locks = Turnstiles::new();
    locks.register(1, Priority::new(100)).expect("register");
    for round in 0..ROUNDS {
        let object = u32::try_from(round).expect("object id") + 1;
        timed("turnstile_uncontended_acquire", || {
            locks.acquire(object, 1).expect("acquire");
        });
        timed("turnstile_uncontended_release", || {
            assert_eq!(locks.wake_one(object), None);
        });
        assert_eq!(locks.owner(object), None);
    }
}

#[test]
fn stress_contended_handoff() {
    for _ in 0..ROUNDS {
        let mut locks = Turnstiles::new();
        locks.register(1, Priority::new(100)).expect("owner");
        locks.register(2, Priority::new(50)).expect("waiter 2");
        locks.register(3, Priority::new(10)).expect("waiter 3");
        locks.register(4, Priority::new(90)).expect("waiter 4");
        locks.acquire(1, 1).expect("acquire");
        for (thread, priority) in [(2, 50), (3, 10), (4, 90)] {
            let _ = priority;
            timed("turnstile_contended_block", || {
                locks.block(1, thread).expect("block");
            });
        }
        assert_eq!(locks.effective_priority(1), Some(Priority::new(10)));
        assert_eq!(
            timed("turnstile_contended_wake", || locks.wake_one(1)),
            Some(3)
        );
        assert_eq!(locks.effective_priority(3), Some(Priority::new(10)));
        assert_eq!(
            timed("turnstile_contended_wake", || locks.wake_one(1)),
            Some(2)
        );
        assert_eq!(
            timed("turnstile_contended_wake", || locks.wake_one(1)),
            Some(4)
        );
        assert_eq!(locks.wake_one(1), None);
        assert_eq!(locks.owner(1), None);
    }
}

#[test]
fn stress_timeout_abort() {
    for _ in 0..ROUNDS {
        let mut locks = Turnstiles::new();
        locks.register(1, Priority::new(100)).expect("owner");
        locks.register(2, Priority::new(10)).expect("waiter");
        locks.acquire(1, 1).expect("acquire");
        locks.block(1, 2).expect("block");
        assert_eq!(locks.effective_priority(1), Some(Priority::new(10)));
        let outcome = timed("turnstile_timeout_abort", || locks.cancel(1, 2));
        assert!(outcome.removed);
        assert_eq!(locks.effective_priority(1), Some(Priority::new(100)));
        assert!(locks.waiters(1).is_empty());
        assert!(!locks.cancel(1, 2).removed);
    }
}

#[test]
fn stress_wake_ordering() {
    let priorities = [40, 10, 80, 20, 70, 30, 60, 50];
    let mut expected: Vec<u32> = (0..priorities.len())
        .map(|index| u32::try_from(index).expect("index") + 2)
        .collect();
    expected.sort_by_key(|thread| priorities[usize::try_from(*thread - 2).expect("thread")]);
    for _ in 0..ROUNDS {
        let mut locks = Turnstiles::new();
        locks.register(1, Priority::new(100)).expect("owner");
        for (index, priority) in priorities.iter().enumerate() {
            locks
                .register(
                    u32::try_from(index).expect("thread") + 2,
                    Priority::new(*priority),
                )
                .expect("waiter");
        }
        locks.acquire(1, 1).expect("acquire");
        for index in 0..priorities.len() {
            let thread = u32::try_from(index).expect("thread") + 2;
            locks.block(1, thread).expect("block");
        }
        let woken = timed("turnstile_wake_all", || locks.wake_all(1));
        assert_eq!(woken, expected);
    }
}

#[test]
fn stress_no_lost_wakeup() {
    const WAITERS: u32 = 32;
    for _ in 0..ROUNDS {
        let mut locks = Turnstiles::new();
        locks.register(1, Priority::new(100)).expect("owner");
        for thread in 2..2 + WAITERS {
            locks
                .register(
                    thread,
                    Priority::new(u8::try_from(thread).expect("priority")),
                )
                .expect("waiter");
        }
        locks.acquire(1, 1).expect("acquire");
        for thread in 2..2 + WAITERS {
            locks.block(1, thread).expect("block");
        }
        let mut seen = Vec::new();
        while let Some(thread) = timed("turnstile_no_lost_wakeup", || locks.wake_one(1)) {
            assert!(!seen.contains(&thread), "duplicate wakeup for {thread}");
            seen.push(thread);
        }
        assert_eq!(seen.len(), usize::try_from(WAITERS).expect("waiters"));
        assert!(locks.waiters(1).is_empty());
    }
}

#[test]
fn stress_priority_inversion_bound() {
    for _ in 0..ROUNDS {
        let mut locks = Turnstiles::new();
        locks.register(1, Priority::new(200)).expect("owner");
        locks.register(2, Priority::new(20)).expect("waiter");
        locks.acquire(1, 1).expect("acquire");
        timed("turnstile_inversion_block", || {
            locks.block(1, 2).expect("block");
        });
        let owner = locks.effective_priority(1).expect("owner priority");
        assert!(
            owner.raw() <= 20,
            "priority inversion: owner effective {} > waiter 20",
            owner.raw()
        );
        timed("turnstile_inversion_restore", || {
            let _ = locks.cancel(1, 2);
        });
        assert_eq!(locks.effective_priority(1), Some(Priority::new(200)));
    }
}
