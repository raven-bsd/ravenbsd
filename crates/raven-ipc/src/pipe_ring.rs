// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fixed-capacity byte ring shared by the kernel pipe primitive and its host tests.
//!
//! The ring only moves bytes. Endpoints, blocking, user copies and handle installation live in
//! `kernel/src/process/pipe.rs`. Both the kernel and `raven-ipc` compile this exact source, so the
//! host tests exercise the same wrap arithmetic the kernel runs.

use alloc::vec::Vec;

/// Capacity of one kernel pipe in bytes.
pub const PIPE_CAPACITY: usize = 64 * 1024;

/// A byte ring with one stable storage allocation.
pub struct Ring {
    bytes: Vec<u8>,
    head: usize,
    len: usize,
}

impl Ring {
    /// Allocate a ring of `capacity` bytes, or `None` when the allocation fails.
    pub fn new(capacity: usize) -> Option<Self> {
        if capacity == 0 {
            return None;
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(capacity).ok()?;
        bytes.resize(capacity, 0);
        Some(Self {
            bytes,
            head: 0,
            len: 0,
        })
    }

    /// Allocate the standard 64 KiB pipe ring.
    pub fn pipe() -> Option<Self> {
        Self::new(PIPE_CAPACITY)
    }

    /// Storage capacity in bytes.
    pub fn capacity(&self) -> usize {
        self.bytes.len()
    }

    /// Number of readable bytes.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when no byte is readable.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Number of writable bytes.
    pub fn free(&self) -> usize {
        self.capacity().saturating_sub(self.len)
    }

    /// Readable bytes in FIFO order: `(head_to_end, wrapped_start)`.
    pub fn read_parts(&self) -> (&[u8], &[u8]) {
        let capacity = self.capacity();
        let first_len = self.len.min(capacity.saturating_sub(self.head));
        let first_end = self.head.saturating_add(first_len);
        let first = self.bytes.get(self.head..first_end).unwrap_or(&[]);
        let second_len = self.len.saturating_sub(first_len);
        let second = self.bytes.get(..second_len).unwrap_or(&[]);
        (first, second)
    }

    /// Writable bytes in write order: `(tail_to_end, wrapped_start)`.
    pub fn write_parts(&mut self) -> (&mut [u8], &mut [u8]) {
        let capacity = self.capacity();
        let tail = self.head.saturating_add(self.len) % capacity;
        let free = self.free();
        let (before, after) = self.bytes.split_at_mut(tail);
        let first_len = free.min(after.len());
        let second_len = free.saturating_sub(first_len);
        let (first, _) = after.split_at_mut(first_len);
        let (second, _) = before.split_at_mut(second_len);
        (first, second)
    }

    /// Drop `count` readable bytes from the front, clamped to the current length.
    pub fn consume(&mut self, count: usize) {
        let count = count.min(self.len);
        if count == 0 {
            return;
        }
        let capacity = self.capacity();
        self.head = self.head.saturating_add(count) % capacity;
        self.len -= count;
    }

    /// Publish `count` written bytes, clamped to the current free space.
    pub fn produce(&mut self, count: usize) {
        let count = count.min(self.free());
        self.len = self.len.saturating_add(count);
    }
}
