// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Lock-free per-CPU ring buffers.
//!
//! Each ring has exactly one producer (the CPU whose identifier selects it) and one consumer (the
//! kernel serialises `DTRACE_READ` with the provider lock). `head` is the producer index, `tail`
//! the consumer index; a full ring drops the new record and increments `dropped` rather than
//! overwriting a record the consumer has not seen.

use core::cell::UnsafeCell;
use core::mem::size_of;
use core::sync::atomic::{AtomicU64, Ordering};

/// One trace record.
///
/// The layout is the wire format shared by the kernel and the consumer; the fixed size lets a
/// consumer index a buffer without per-record metadata.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// Monotonic record sequence number (assigned before the push).
    pub sequence: u64,
    /// Kernel monotonic timestamp in nanoseconds.
    pub timestamp_ns: u64,
    /// Probe argument 0.
    pub arg0: u64,
    /// Probe argument 1.
    pub arg1: u64,
    /// Probe argument 2.
    pub arg2: u64,
    /// Probe argument 3.
    pub arg3: u64,
    /// Native syscall number for the syscall provider (zero for other providers).
    pub syscall: u32,
    /// Thread identifier of the probe site.
    pub tid: u32,
    /// CPU index that produced the record.
    pub cpu: u16,
    /// Probe identifier.
    pub probe: u16,
}

const _: () = assert!(size_of::<Record>() == 64);

/// Per-CPU ring statistics.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuStats {
    /// Records accepted by the ring.
    pub produced: u64,
    /// Records handed to the consumer.
    pub consumed: u64,
    /// Records refused because the ring was full.
    pub dropped: u64,
    /// Ring capacity in records.
    pub capacity: u64,
}

const _: () = assert!(size_of::<CpuStats>() == 32);

/// A fixed-capacity single-producer/single-consumer ring.
///
/// `N` must be a power of two. The producer and consumer may run on different CPUs; the
/// Acquire/Release pairs on `head`/`tail` publish the slot write and the slot release.
pub struct Ring<const N: usize> {
    slots: UnsafeCell<[Record; N]>,
    head: AtomicU64,
    tail: AtomicU64,
    produced: AtomicU64,
    consumed: AtomicU64,
    dropped: AtomicU64,
}

// SAFETY: the ring is a single-producer/single-consumer queue. The producer only writes a slot
// whose distance from `tail` is below `N`; the consumer only reads the slot at `tail` after
// observing `head` with Acquire. `head`, `tail` and the counters are atomics, so the shared
// `UnsafeCell` slot array is never accessed concurrently for the same slot.
unsafe impl<const N: usize> Sync for Ring<N> {}

impl<const N: usize> Ring<N> {
    /// Index mask; `N` is a power of two.
    const MASK: u64 = (N as u64).wrapping_sub(1);

    /// An empty ring.
    pub const fn new() -> Self {
        assert!(N.is_power_of_two());
        Self {
            slots: UnsafeCell::new(
                [Record {
                    sequence: 0,
                    timestamp_ns: 0,
                    arg0: 0,
                    arg1: 0,
                    arg2: 0,
                    arg3: 0,
                    syscall: 0,
                    tid: 0,
                    cpu: 0,
                    probe: 0,
                }; N],
            ),
            head: AtomicU64::new(0),
            tail: AtomicU64::new(0),
            produced: AtomicU64::new(0),
            consumed: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    /// Push one record; false when the ring is full (the record is dropped).
    pub fn push(&self, record: Record) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= N as u64 {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let index = (head & Self::MASK) as usize;
        // SAFETY: single producer; the slot at `head & MASK` is not reachable by the consumer
        // while head - tail < N, and no other producer writes it.
        unsafe {
            self.slots.get().cast::<Record>().add(index).write(record);
        }
        self.head.store(head.wrapping_add(1), Ordering::Release);
        self.produced.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Pop the oldest record, if any.
    pub fn pop(&self) -> Option<Record> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail == head {
            return None;
        }
        let index = (tail & Self::MASK) as usize;
        // SAFETY: single consumer; the slot at `tail & MASK` was published before the Acquire
        // load of `head` and is not written again until tail advances by N.
        let record = unsafe { self.slots.get().cast::<Record>().add(index).read() };
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        self.consumed.fetch_add(1, Ordering::Relaxed);
        Some(record)
    }

    /// True when no record is waiting.
    pub fn is_empty(&self) -> bool {
        self.tail.load(Ordering::Acquire) == self.head.load(Ordering::Acquire)
    }

    /// Current statistics.
    pub fn stats(&self) -> CpuStats {
        CpuStats {
            produced: self.produced.load(Ordering::Relaxed),
            consumed: self.consumed.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            capacity: N as u64,
        }
    }

    /// Reset the ring and its counters.
    ///
    /// The caller must guarantee that no producer or consumer is active; the kernel holds the
    /// provider lock and clears records only after disabling every probe.
    pub fn reset(&self) {
        self.head.store(0, Ordering::Release);
        self.tail.store(0, Ordering::Release);
        self.produced.store(0, Ordering::Relaxed);
        self.consumed.store(0, Ordering::Relaxed);
        self.dropped.store(0, Ordering::Relaxed);
    }
}

impl<const N: usize> Default for Ring<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "ring_ut.rs"]
mod tests;
