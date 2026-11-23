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
/// Hard bound on live threads in the kernel (K527).
///
/// Each thread can be a member of at most one run-queue level on one CPU, so a per-level
/// capacity equal to this bound can never fill: a level can hold no more entries than the
/// number of live threads. The kernel enforces the same bound at thread creation
/// (`kernel/src/sched/thread.rs`), so a runnable thread can never be refused a slot.
pub const MAX_THREADS: usize = 1024;
/// Maximum threads in one priority FIFO.
///
/// Deliberately equal to [`MAX_THREADS`]: the queue is the fixed-capacity, allocation-free
/// admission gate, and its bound must cover every thread that can target one level.
pub const CAPACITY: usize = MAX_THREADS;

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

    /// Iterate the live entries (S36 holder diagnostic).
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.slots.iter().filter_map(Option::as_ref)
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

    /// Iterate the sleeping values (S36 holder diagnostic).
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().map(|entry| &entry.value)
    }
}

impl<T> Default for SleepList<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
