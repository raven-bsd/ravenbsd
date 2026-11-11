// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Lock-free single-producer/single-consumer byte ring.
//!
//! The ring keeps the newest [`CAPACITY`] unread bytes.  One producer appends with [`RxRing::push`]
//! and one consumer drains with [`RxRing::take`]; no lock is involved and every method takes
//! `&self`.  Indices are free-running counters (masked for indexing), so the buffer needs no
//! sentinel slot and its capacity is exactly [`CAPACITY`].
//!
//! # Overrun policy
//!
//! When the producer laps the consumer, the oldest bytes are overwritten.  The consumer detects
//! the gap from the counter distance and skips those bytes, adding them to [`RxRing::dropped`];
//! it always returns the newest data, never stale bytes.
//!
//! # Memory ordering
//!
//! The producer stores the payload with `Relaxed` and then publishes the new tail with `Release`.
//! The consumer loads the tail with `Acquire`, reads only slots below that snapshot, and publishes
//! its head with `Release`.  Each index has exactly one writer, so plain load/store is sufficient.
//!
//! # Safety
//!
//! The crate is safe Rust: payload slots are `AtomicU8`, so a concurrent overwrite cannot tear a
//! byte.  `Send`/`Sync` follow from the atomics; the type is meant to live in a `static`.

#![cfg_attr(not(target_os = "none"), no_std)]
#![forbid(unsafe_code)]
// The kernel includes this crate as a module and uses only part of its API.
#![cfg_attr(target_os = "none", allow(dead_code))]

use core::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};

/// Number of bytes the ring retains.
pub const CAPACITY: usize = 4096;

/// A single-producer/single-consumer byte ring.
pub struct RxRing {
    slots: [AtomicU8; CAPACITY],
    /// Monotonic number of bytes the producer has pushed.
    tail: AtomicUsize,
    /// Monotonic number of bytes the consumer has popped or skipped.
    head: AtomicUsize,
    /// Bytes lost to overrun (counted by the consumer when it detects the gap).
    dropped: AtomicU64,
}

impl RxRing {
    /// Create an empty ring.
    pub const fn new() -> Self {
        Self {
            slots: [const { AtomicU8::new(0) }; CAPACITY],
            tail: AtomicUsize::new(0),
            head: AtomicUsize::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    /// Append one byte, overwriting the oldest unread byte when the ring is full.
    ///
    /// Producer side only; never blocks and never allocates.
    pub fn push(&self, byte: u8) {
        let tail = self.tail.load(Ordering::Relaxed);
        let slot = tail.wrapping_rem(CAPACITY);
        if let Some(slot) = self.slots.get(slot) {
            slot.store(byte, Ordering::Relaxed);
        }
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
    }

    /// Drain up to `out.len()` bytes into `out` and return the count.
    ///
    /// Consumer side only.  Returns 0 when the ring is empty.  If the producer overran the ring,
    /// the lost prefix is counted in [`RxRing::dropped`] and the newest bytes are returned.
    pub fn take(&self, out: &mut [u8]) -> usize {
        let mut head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        let mut lag = tail.wrapping_sub(head);
        if lag > CAPACITY {
            let skipped = lag.wrapping_sub(CAPACITY);
            head = head.wrapping_add(skipped);
            self.dropped.fetch_add(
                u64::try_from(skipped).unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
            lag = CAPACITY;
        }
        let count = lag.min(out.len());
        let mut index = 0usize;
        while index < count {
            let slot = head.wrapping_add(index).wrapping_rem(CAPACITY);
            let byte = self
                .slots
                .get(slot)
                .map_or(0, |slot| slot.load(Ordering::Relaxed));
            if let Some(destination) = out.get_mut(index) {
                *destination = byte;
            }
            index = index.saturating_add(1);
        }
        self.head.store(head.wrapping_add(count), Ordering::Release);
        count
    }

    /// Remove and return the oldest unread byte, or `None` when empty.
    ///
    /// Consumer side only; convenience wrapper for single-byte readers.
    pub fn pop(&self) -> Option<u8> {
        let mut byte = [0u8; 1];
        match self.take(&mut byte) {
            1 => byte.first().copied(),
            _ => None,
        }
    }

    /// Number of unread bytes (capped at [`CAPACITY`]).
    pub fn len(&self) -> usize {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);
        tail.wrapping_sub(head).min(CAPACITY)
    }

    /// True when no byte is waiting.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total bytes lost to overrun since creation.
    ///
    /// The counter is updated by the consumer when it observes the gap, so it is exact at the
    /// moment a drain reports it.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Retained capacity in bytes.
    pub const fn capacity(&self) -> usize {
        CAPACITY
    }
}

impl Default for RxRing {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_str(ring: &RxRing, text: &[u8]) {
        for byte in text {
            ring.push(*byte);
        }
    }

    #[test]
    fn new_ring_is_empty() {
        let ring = RxRing::new();
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);
        assert_eq!(ring.dropped(), 0);
        assert_eq!(ring.capacity(), CAPACITY);
    }

    #[test]
    fn push_then_pop_roundtrips_one_byte() {
        let ring = RxRing::new();
        ring.push(0x42);
        assert_eq!(ring.len(), 1);
        assert_eq!(ring.pop(), Some(0x42));
        assert!(ring.is_empty());
        assert_eq!(ring.pop(), None);
    }

    #[test]
    fn fifo_order_is_preserved() {
        let ring = RxRing::new();
        push_str(&ring, b"hello");
        let mut out = [0u8; 8];
        assert_eq!(ring.take(&mut out), 5);
        assert_eq!(&out[..5], b"hello");
        assert_eq!(ring.take(&mut out), 0);
    }

    #[test]
    fn wrap_around_keeps_order() {
        let ring = RxRing::new();
        let mut expected = 0u16;
        let mut received = 0u16;
        // Push and drain in small interleaved steps so both indices wrap several times.
        while expected < 10_000 {
            push_str(&ring, &[(expected & 0xff) as u8]);
            expected = expected.saturating_add(1);
            if expected.is_multiple_of(3) {
                let mut out = [0u8; 4];
                let count = ring.take(&mut out);
                for byte in out.iter().take(count) {
                    assert_eq!(*byte, (received & 0xff) as u8);
                    received = received.saturating_add(1);
                }
            }
        }
        let mut out = [0u8; 8];
        loop {
            let count = ring.take(&mut out);
            if count == 0 {
                break;
            }
            for byte in out.iter().take(count) {
                assert_eq!(*byte, (received & 0xff) as u8);
                received = received.saturating_add(1);
            }
        }
        assert_eq!(received, expected);
        assert_eq!(ring.dropped(), 0);
    }

    #[test]
    fn fill_to_capacity_is_not_full_loss() {
        let ring = RxRing::new();
        for index in 0..CAPACITY {
            ring.push((index & 0xff) as u8);
        }
        assert_eq!(ring.len(), CAPACITY);
        assert!(!ring.is_empty());
        assert_eq!(ring.dropped(), 0);
        let mut out = [0u8; CAPACITY];
        assert_eq!(ring.take(&mut out), CAPACITY);
        for (index, byte) in out.iter().enumerate() {
            assert_eq!(*byte, (index & 0xff) as u8);
        }
    }

    #[test]
    fn overrun_drops_the_oldest_bytes() {
        let ring = RxRing::new();
        for index in 0..CAPACITY + 10 {
            ring.push((index & 0xff) as u8);
        }
        // The drop is counted only when the consumer observes the gap.
        let mut out = [0u8; CAPACITY];
        assert_eq!(ring.take(&mut out), CAPACITY);
        assert_eq!(ring.dropped(), 10);
        for (index, byte) in out.iter().enumerate() {
            // The first ten bytes were overwritten; the ring now starts at byte ten.
            let expected = ((index + 10) & 0xff) as u8;
            assert_eq!(*byte, expected, "slot {index} must hold the newest byte");
        }
    }

    #[test]
    fn overrun_by_one_drops_exactly_one() {
        let ring = RxRing::new();
        for index in 0..CAPACITY + 1 {
            ring.push((index & 0xff) as u8);
        }
        let mut out = [0u8; 1];
        assert_eq!(ring.take(&mut out), 1);
        assert_eq!(ring.dropped(), 1);
        // Byte zero was overwritten, so the oldest survivor is byte one.
        assert_eq!(out[0], 1);
    }

    #[test]
    fn take_respects_the_output_length() {
        let ring = RxRing::new();
        push_str(&ring, b"abcdef");
        let mut out = [0u8; 2];
        assert_eq!(ring.take(&mut out), 2);
        assert_eq!(&out, b"ab");
        assert_eq!(ring.len(), 4);
        assert_eq!(ring.dropped(), 0);
    }

    #[test]
    fn take_on_empty_returns_zero() {
        let ring = RxRing::new();
        let mut out = [0u8; 16];
        assert_eq!(ring.take(&mut out), 0);
        assert_eq!(ring.dropped(), 0);
    }

    #[test]
    fn interleaved_push_pop_never_drops() {
        let ring = RxRing::new();
        let mut received = 0u16;
        for expected in 0..5_000u16 {
            ring.push((expected & 0xff) as u8);
            if expected.is_multiple_of(2) {
                let mut out = [0u8; 3];
                let count = ring.take(&mut out);
                for byte in out.iter().take(count) {
                    assert_eq!(*byte, (received & 0xff) as u8);
                    received = received.saturating_add(1);
                }
            }
        }
        let mut out = [0u8; 8];
        loop {
            let count = ring.take(&mut out);
            if count == 0 {
                break;
            }
            for byte in out.iter().take(count) {
                assert_eq!(*byte, (received & 0xff) as u8);
                received = received.saturating_add(1);
            }
        }
        assert_eq!(received, 5_000);
        assert_eq!(ring.dropped(), 0);
    }

    #[test]
    fn dropped_counter_accumulates_across_overruns() {
        let ring = RxRing::new();
        for index in 0..CAPACITY + 3 {
            ring.push((index & 0xff) as u8);
        }
        let mut out = [0u8; CAPACITY];
        let _ = ring.take(&mut out);
        assert_eq!(ring.dropped(), 3);
        for index in 0..CAPACITY + 2 {
            ring.push((index & 0xff) as u8);
        }
        let _ = ring.take(&mut out);
        assert_eq!(ring.dropped(), 5);
    }

    #[test]
    fn one_free_slot_after_drain_avoids_drops() {
        let ring = RxRing::new();
        for index in 0..CAPACITY {
            ring.push((index & 0xff) as u8);
        }
        let mut out = [0u8; 1];
        assert_eq!(ring.take(&mut out), 1);
        ring.push(0xAA);
        assert_eq!(ring.dropped(), 0);
        assert_eq!(ring.len(), CAPACITY);
    }

    #[test]
    fn all_byte_values_roundtrip() {
        let ring = RxRing::new();
        for value in 0..=255u8 {
            ring.push(value);
        }
        let mut out = [0u8; 256];
        assert_eq!(ring.take(&mut out), 256);
        for value in 0..=255u8 {
            assert_eq!(out[value as usize], value);
        }
        assert_eq!(ring.dropped(), 0);
    }

    #[test]
    fn len_is_capped_after_overrun() {
        let ring = RxRing::new();
        for index in 0..CAPACITY + 5 {
            ring.push((index & 0xff) as u8);
        }
        assert_eq!(ring.len(), CAPACITY);
        assert_eq!(ring.dropped(), 0, "the gap is observed by the consumer");
    }
}
