// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure scheduler data structures.
//!
//! The run queue is a fixed-capacity 32-level FIFO set with a bitmap. Its storage is allocated
//! once when the queue is created, so the timer interrupt can enqueue a woken thread without
//! allocating. The sleep list is sorted by deadline and drains
//! expired entries through a callback for the same reason.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;

/// Number of priority levels (0 highest, 31 lowest).
pub const LEVELS: usize = 32;
/// Maximum threads in one priority FIFO.
pub const CAPACITY: usize = 64;

/// A 32-priority FIFO run queue.
///
/// The backing storage is allocated once by [`RunQueue::new`] or [`RunQueue::with_capacity`];
/// `push_back` and `pop_front` never allocate, so the timer ISR can wake a thread safely.
pub struct RunQueue<T> {
    slots: Vec<Option<T>>,
    head: [usize; LEVELS],
    len: [usize; LEVELS],
    bitmap: u32,
}

impl<T> RunQueue<T> {
    /// Create an empty queue with [`CAPACITY`] slots per priority.
    pub fn new() -> Self {
        Self::with_capacity(CAPACITY)
    }

    /// Create an empty queue with at most `capacity` slots per priority (clamped to `1..=CAPACITY`).
    pub fn with_capacity(capacity: usize) -> Self {
        let per_level = capacity.clamp(1, CAPACITY);
        let total = LEVELS.saturating_mul(per_level);
        let mut slots = Vec::with_capacity(total);
        slots.resize_with(total, || None);
        Self {
            slots,
            head: [0; LEVELS],
            len: [0; LEVELS],
            bitmap: 0,
        }
    }

    fn level_capacity(&self) -> usize {
        self.slots.len() / LEVELS
    }

    /// Append `value` at `priority`; returns it back when the level is full or invalid.
    pub fn push_back(&mut self, priority: u8, value: T) -> Result<(), T> {
        let priority = usize::from(priority);
        if priority >= LEVELS {
            return Err(value);
        }
        let capacity = self.level_capacity();
        let length = self.len[priority];
        if length >= capacity {
            return Err(value);
        }
        let tail = self.head[priority].saturating_add(length) % capacity;
        let index = priority.saturating_mul(capacity).saturating_add(tail);
        let Some(slot) = self.slots.get_mut(index) else {
            return Err(value);
        };
        *slot = Some(value);
        self.len[priority] = length.saturating_add(1);
        self.bitmap |= 1u32 << priority;
        Ok(())
    }

    /// Pop the oldest value at the highest-priority non-empty level.
    pub fn pop_front(&mut self) -> Option<T> {
        if self.bitmap == 0 {
            return None;
        }
        let priority = self.bitmap.trailing_zeros() as usize;
        let capacity = self.level_capacity();
        let head = self.head[priority];
        let length = self.len[priority];
        let index = priority.saturating_mul(capacity).saturating_add(head);
        let slot = self.slots.get_mut(index)?;
        let value = slot.take()?;
        self.head[priority] = head.saturating_add(1) % capacity;
        let new_length = length.saturating_sub(1);
        self.len[priority] = new_length;
        if new_length == 0 {
            self.bitmap &= !(1u32 << priority);
        }
        Some(value)
    }

    /// True when no thread is ready.
    pub fn is_empty(&self) -> bool {
        self.bitmap == 0
    }

    /// Number of ready threads.
    pub fn len(&self) -> usize {
        self.len.iter().sum()
    }

    /// Number of threads at one priority.
    pub fn level_len(&self, priority: u8) -> usize {
        self.len.get(usize::from(priority)).copied().unwrap_or(0)
    }
}

impl<T> Default for RunQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// One sleeping thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SleepEntry<T> {
    /// Wake-up deadline in monotonic nanoseconds.
    pub deadline: u64,
    /// The sleeping value.
    pub value: T,
}

/// A deadline-ordered sleep list.
pub struct SleepList<T> {
    entries: Vec<SleepEntry<T>>,
}

impl<T> SleepList<T> {
    /// Create an empty list.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Create an empty list whose backing storage can hold `capacity` entries without allocating.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }

    /// Insert keeping the list sorted by deadline; equal deadlines retain insertion order.
    pub fn insert(&mut self, deadline: u64, value: T) {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.deadline > deadline)
            .unwrap_or(self.entries.len());
        self.entries.insert(index, SleepEntry { deadline, value });
    }

    /// Remove and return the first entry whose value matches `predicate`.
    pub fn remove_where(&mut self, mut predicate: impl FnMut(&T) -> bool) -> Option<T> {
        let mut index = 0usize;
        while index < self.entries.len() {
            let matches = self
                .entries
                .get(index)
                .is_some_and(|entry| predicate(&entry.value));
            if matches {
                return Some(self.entries.remove(index).value);
            }
            index = index.saturating_add(1);
        }
        None
    }

    /// Drain every entry with `deadline <= now` through `sink`.
    pub fn drain_expired(&mut self, now: u64, mut sink: impl FnMut(T)) {
        let mut index = 0usize;
        while index < self.entries.len() {
            let expired = self
                .entries
                .get(index)
                .is_some_and(|entry| entry.deadline <= now);
            if expired {
                sink(self.entries.remove(index).value);
            } else {
                index = index.saturating_add(1);
            }
        }
    }

    /// Remove and return the earliest entry when its deadline is at or before `now`.
    ///
    /// This is the timer-ISR path: `Vec::remove` never allocates.
    pub fn pop_expired(&mut self, now: u64) -> Option<T> {
        let first = self.entries.first()?;
        if first.deadline > now {
            return None;
        }
        Some(self.entries.remove(0).value)
    }

    /// Number of sleepers.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no thread is sleeping.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<T> Default for SleepList<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
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
}
