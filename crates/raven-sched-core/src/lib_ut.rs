// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the run queue and sleep list.

use super::*;
use alloc::collections::{BTreeMap, VecDeque};

#[test]
fn runqueue_priority_order() {
    let mut queue = RunQueue::new();
    queue.push_back(10, 1).unwrap();
    queue.push_back(3, 2).unwrap();
    queue.push_back(31, 3).unwrap();
    assert_eq!(queue.pop_front(), Some(2));
    assert_eq!(queue.pop_front(), Some(1));
    assert_eq!(queue.pop_front(), Some(3));
}

#[test]
fn runqueue_fifo_within_level() {
    let mut queue = RunQueue::new();
    queue.push_back(7, "a").unwrap();
    queue.push_back(7, "b").unwrap();
    queue.push_back(7, "c").unwrap();
    assert_eq!(queue.pop_front(), Some("a"));
    assert_eq!(queue.pop_front(), Some("b"));
    assert_eq!(queue.pop_front(), Some("c"));
}

#[test]
fn runqueue_empty() {
    let mut queue: RunQueue<u8> = RunQueue::new();
    assert!(queue.is_empty());
    assert_eq!(queue.pop_front(), None);
}

#[test]
fn runqueue_invalid_priority_returns_value() {
    let mut queue = RunQueue::new();
    assert_eq!(queue.push_back(32, 9), Err(9));
    assert!(queue.is_empty());
}

#[test]
fn runqueue_level_capacity() {
    let mut queue = RunQueue::new();
    for value in 0..CAPACITY {
        assert!(queue.push_back(4, value).is_ok());
    }
    assert_eq!(queue.push_back(4, 99), Err(99));
    assert_eq!(queue.level_len(4), CAPACITY);
}

#[test]
fn runqueue_len_and_bitmap_clear() {
    let mut queue = RunQueue::new();
    queue.push_back(1, 1).unwrap();
    queue.push_back(2, 2).unwrap();
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.pop_front(), Some(1));
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.pop_front(), Some(2));
    assert!(queue.is_empty());
    assert_eq!(queue.len(), 0);
}

#[test]
fn runqueue_wraps_ring() {
    let mut queue = RunQueue::new();
    for value in 0..CAPACITY {
        queue.push_back(0, value).unwrap();
    }
    for _ in 0..CAPACITY / 2 {
        assert!(queue.pop_front().is_some());
    }
    for value in 0..CAPACITY / 2 {
        queue.push_back(0, 100 + value).unwrap();
    }
    for value in CAPACITY / 2..CAPACITY {
        assert_eq!(queue.pop_front(), Some(value));
    }
    for value in 0..CAPACITY / 2 {
        assert_eq!(queue.pop_front(), Some(100 + value));
    }
}

#[test]
fn runqueue_random_model() {
    let mut queue = RunQueue::new();
    let mut model: BTreeMap<u8, VecDeque<u32>> = BTreeMap::new();
    let mut state = 0x1234_5678u32;
    let mut next_value = 0u32;
    for _ in 0..4000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let model_empty = model.values().all(VecDeque::is_empty);
        let model_len: usize = model.values().map(VecDeque::len).sum();
        if (state & 1 == 0 || model_empty) && model_len < 12 {
            let priority = ((state >> 1) % 32) as u8;
            assert!(queue.push_back(priority, next_value).is_ok());
            model.entry(priority).or_default().push_back(next_value);
            next_value = next_value.wrapping_add(1);
        } else if let Some(priority) = model.keys().next().copied() {
            let expected = model
                .get_mut(&priority)
                .and_then(VecDeque::pop_front)
                .unwrap();
            assert_eq!(queue.pop_front(), Some(expected));
            if model.get(&priority).is_some_and(VecDeque::is_empty) {
                model.remove(&priority);
            }
        }
    }
    while let Some(priority) = model.keys().next().copied() {
        let expected = model
            .get_mut(&priority)
            .and_then(VecDeque::pop_front)
            .unwrap();
        assert_eq!(queue.pop_front(), Some(expected));
        if model.get(&priority).is_some_and(VecDeque::is_empty) {
            model.remove(&priority);
        }
    }
    assert!(queue.is_empty());
}

#[test]
fn sleep_orders_deadlines() {
    let mut list = SleepList::new();
    list.insert(30, "c");
    list.insert(10, "a");
    list.insert(20, "b");
    let mut order = Vec::new();
    list.drain_expired(25, |value| order.push(value));
    assert_eq!(order, alloc::vec!["a", "b"]);
    assert_eq!(list.len(), 1);
}

#[test]
fn sleep_equal_deadlines_keep_insertion_order() {
    let mut list = SleepList::new();
    list.insert(10, 1);
    list.insert(10, 2);
    let mut order = Vec::new();
    list.drain_expired(10, |value| order.push(value));
    assert_eq!(order, alloc::vec![1, 2]);
}

#[test]
fn sleep_not_expired_stays() {
    let mut list = SleepList::new();
    list.insert(100, 1);
    list.drain_expired(99, |_| panic!("must not wake"));
    assert_eq!(list.len(), 1);
}

#[test]
fn sleep_remove_where() {
    let mut list = SleepList::new();
    list.insert(10, 1);
    list.insert(20, 2);
    assert_eq!(list.remove_where(|value| *value == 2), Some(2));
    assert_eq!(list.remove_where(|value| *value == 2), None);
    assert_eq!(list.len(), 1);
}

#[test]
fn sleep_empty_default() {
    let list: SleepList<u8> = SleepList::default();
    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
}

#[test]
fn runqueue_level_len_invalid_priority_is_zero() {
    let queue: RunQueue<u8> = RunQueue::new();
    assert_eq!(queue.level_len(32), 0);
    assert_eq!(queue.level_len(255), 0);
}

#[test]
fn runqueue_high_bitmap_bit_wins() {
    let mut queue = RunQueue::new();
    queue.push_back(31, 1).unwrap();
    queue.push_back(0, 2).unwrap();
    queue.push_back(15, 3).unwrap();
    assert_eq!(queue.pop_front(), Some(2));
    assert_eq!(queue.pop_front(), Some(3));
    assert_eq!(queue.pop_front(), Some(1));
}

#[test]
fn runqueue_reuses_slots_after_wrap() {
    let mut queue = RunQueue::new();
    for round in 0..8u32 {
        for value in 0..CAPACITY {
            assert!(queue.push_back(5, round * 100 + value as u32).is_ok());
        }
        for value in 0..CAPACITY {
            assert_eq!(queue.pop_front(), Some(round * 100 + value as u32));
        }
        assert!(queue.is_empty());
    }
}

#[test]
fn runqueue_small_capacity_wraps() {
    let mut queue = RunQueue::with_capacity(2);
    assert!(queue.push_back(3, 1).is_ok());
    assert!(queue.push_back(3, 2).is_ok());
    assert_eq!(queue.push_back(3, 3), Err(3));
    assert_eq!(queue.pop_front(), Some(1));
    assert!(queue.push_back(3, 4).is_ok());
    assert_eq!(queue.pop_front(), Some(2));
    assert_eq!(queue.pop_front(), Some(4));
    assert!(queue.is_empty());
}

#[test]
fn runqueue_failed_push_keeps_state() {
    let mut queue = RunQueue::new();
    for value in 0..CAPACITY {
        queue.push_back(2, value).unwrap();
    }
    assert_eq!(queue.push_back(2, 999), Err(999));
    assert_eq!(queue.level_len(2), CAPACITY);
    assert_eq!(queue.len(), CAPACITY);
    for value in 0..CAPACITY {
        assert_eq!(queue.pop_front(), Some(value));
    }
}

#[test]
fn runqueue_two_levels_interleave() {
    let mut queue = RunQueue::new();
    queue.push_back(2, 20).unwrap();
    queue.push_back(1, 10).unwrap();
    queue.push_back(2, 21).unwrap();
    queue.push_back(1, 11).unwrap();
    assert_eq!(queue.pop_front(), Some(10));
    assert_eq!(queue.pop_front(), Some(11));
    assert_eq!(queue.pop_front(), Some(20));
    assert_eq!(queue.pop_front(), Some(21));
}

#[test]
fn sleep_zero_deadline_wakes_at_now() {
    let mut list = SleepList::new();
    list.insert(0, "zero");
    let mut order = Vec::new();
    list.drain_expired(0, |value| order.push(value));
    assert_eq!(order, alloc::vec!["zero"]);
    assert!(list.is_empty());
}

#[test]
fn sleep_insert_in_middle_then_remove() {
    let mut list = SleepList::new();
    list.insert(10, 1);
    list.insert(40, 4);
    list.insert(20, 2);
    list.insert(30, 3);
    assert_eq!(list.remove_where(|value| *value == 3), Some(3));
    assert_eq!(list.remove_where(|value| *value == 2), Some(2));
    let mut order = Vec::new();
    list.drain_expired(100, |value| order.push(value));
    assert_eq!(order, alloc::vec![1, 4]);
}

#[test]
fn sleep_keeps_later_entries_after_partial_drain() {
    let mut list = SleepList::new();
    for value in 0..20u32 {
        list.insert(u64::from(value) * 10, value);
    }
    let mut woken = Vec::new();
    list.drain_expired(95, |value| woken.push(value));
    assert_eq!(woken, (0..10).collect::<Vec<_>>());
    assert_eq!(list.len(), 10);
    let mut rest = Vec::new();
    list.drain_expired(u64::MAX, |value| rest.push(value));
    assert_eq!(rest, (10..20).collect::<Vec<_>>());
    assert!(list.is_empty());
}

#[test]
fn sleep_pop_expired_one_at_a_time() {
    let mut list = SleepList::with_capacity(4);
    list.insert(10, 1);
    list.insert(20, 2);
    assert_eq!(list.pop_expired(5), None);
    assert_eq!(list.pop_expired(10), Some(1));
    assert_eq!(list.pop_expired(19), None);
    assert_eq!(list.pop_expired(20), Some(2));
    assert_eq!(list.pop_expired(u64::MAX), None);
    assert!(list.is_empty());
}

#[test]
fn runqueue_max_priority_and_len_bookkeeping() {
    let mut queue = RunQueue::new();
    assert!(queue.push_back(0, "high").is_ok());
    assert!(queue.push_back(31, "low").is_ok());
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.level_len(0), 1);
    assert_eq!(queue.level_len(31), 1);
    assert_eq!(queue.pop_front(), Some("high"));
    assert_eq!(queue.level_len(0), 0);
    assert_eq!(queue.len(), 1);
}

// --- K527: the per-level bound must cover every thread that can target one level ---

#[test]
fn runqueue_bound_covers_the_thread_bound() {
    // The admission proof: one thread belongs to at most one level, so the kernel-wide thread
    // bound is also the per-level worst case.
    assert_eq!(CAPACITY, MAX_THREADS);
    const _: () = assert!(CAPACITY >= MAX_THREADS);
}

#[test]
fn runqueue_boundary_64_and_65_with_injected_capacity() {
    // The historic 64-slot boundary is still exercised by the test-only smaller queue. A failed
    // 65th admission returns the value and leaves all 64 entries intact (no silent drop).
    let mut queue: RunQueue<u32> = RunQueue::with_capacity(64);
    for value in 0..64 {
        assert!(queue.push_back(9, value).is_ok());
    }
    assert_eq!(queue.push_back(9, 64), Err(64));
    assert_eq!(queue.level_len(9), 64);
    assert_eq!(queue.len(), 64);
    assert_eq!(queue.pop_front(), Some(0));
    assert!(queue.push_back(9, 64).is_ok());
    assert_eq!(queue.level_len(9), 64);
    for expected in 1..64 {
        assert_eq!(queue.pop_front(), Some(expected));
    }
    assert_eq!(queue.pop_front(), Some(64));
    assert!(queue.is_empty());
}

#[test]
fn runqueue_chosen_bound_1024_full_and_1025() {
    let mut queue: RunQueue<u32> = RunQueue::new();
    for value in 0..MAX_THREADS as u32 {
        assert!(queue.push_back(5, value).is_ok(), "slot {value}");
    }
    assert_eq!(queue.level_len(5), MAX_THREADS);
    assert_eq!(queue.len(), MAX_THREADS);
    assert_eq!(queue.push_back(5, u32::MAX), Err(u32::MAX));
    assert_eq!(queue.level_len(5), MAX_THREADS);
    for value in 0..MAX_THREADS as u32 {
        assert_eq!(queue.pop_front(), Some(value));
    }
    assert!(queue.is_empty());
}

#[test]
fn runqueue_with_capacity_is_clamped_to_the_bound() {
    let queue: RunQueue<u8> = RunQueue::with_capacity(MAX_THREADS + 100);
    assert_eq!(queue.level_capacity(), MAX_THREADS);
    let queue: RunQueue<u8> = RunQueue::with_capacity(0);
    assert_eq!(queue.level_capacity(), 1);
}

#[test]
fn runqueue_accounting_has_no_duplicate_membership() {
    // Model the kernel handoff: a thread is only Ready after a successful push, and a pop removes
    // it from the queue. The accounting identity live = queued + elsewhere holds through a
    // saturated level and a drain.
    let mut queue: RunQueue<u32> = RunQueue::with_capacity(4);
    let mut live = 0u32;
    let mut queued = 0u32;
    for value in 0..4 {
        if queue.push_back(3, value).is_ok() {
            live += 1;
            queued += 1;
        }
    }
    assert_eq!(queued, 4);
    assert_eq!(queue.push_back(3, 4), Err(4)); // not admitted: live/queued unchanged
    assert_eq!(live, 4);
    assert_eq!(queued, queue.len() as u32);
    while queue.pop_front().is_some() {
        queued -= 1;
        live -= 1;
    }
    assert_eq!(live, 0);
    assert_eq!(queued, 0);
}

#[test]
fn runqueue_iter_visits_live_entries_only() {
    let mut queue: RunQueue<u32> = RunQueue::with_capacity(4);
    queue.push_back(2, 10).unwrap();
    queue.push_back(0, 11).unwrap();
    queue.push_back(2, 12).unwrap();
    assert_eq!(queue.pop_front(), Some(11));
    let mut seen: Vec<u32> = queue.iter().copied().collect();
    seen.sort_unstable();
    assert_eq!(seen, [10, 12]);
}

#[test]
fn sleep_iter_visits_entries_in_deadline_order() {
    let mut list: SleepList<u32> = SleepList::with_capacity(4);
    list.insert(30, 3);
    list.insert(10, 1);
    list.insert(20, 2);
    let seen: Vec<u32> = list.iter().copied().collect();
    assert_eq!(seen, [1, 2, 3]);
}
