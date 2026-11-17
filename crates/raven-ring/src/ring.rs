// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The ring and its descriptor.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// A ring descriptor: a 63-bit value plus a cancelled bit, packed into one word.
///
/// The kernel integration (Q6) maps the wire `SqEntry`/`CqEntry` onto consecutive words; this
/// pure model keeps one word per descriptor so the slot storage is a plain `[AtomicU64]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Descriptor(u64);

impl Descriptor {
    /// The bit that marks a descriptor cancelled.
    const CANCEL_BIT: u64 = 1 << 63;
    /// Everything below the cancel bit.
    const VALUE_MASK: u64 = (1 << 63) - 1;

    /// A live descriptor carrying `value` (truncated to 63 bits).
    pub const fn new(value: u64) -> Self {
        Self(value & Self::VALUE_MASK)
    }

    /// A cancelled descriptor carrying `value` (truncated to 63 bits).
    pub const fn cancelled(value: u64) -> Self {
        Self(Self::new(value).0 | Self::CANCEL_BIT)
    }

    /// The 63-bit value.
    pub const fn value(self) -> u64 {
        self.0 & Self::VALUE_MASK
    }

    /// Whether this descriptor is cancelled.
    pub const fn is_cancelled(self) -> bool {
        self.0 & Self::CANCEL_BIT != 0
    }

    /// The raw one-word encoding.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Interpret a raw word as a descriptor.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

/// Why a [`Ring`] operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The storage length is zero or not a power of two.
    InvalidCapacity,
    /// The ring is full.
    Full,
    /// The ring has been poisoned.
    Poisoned,
}

impl Error {
    /// A stable short name for logs and telemetry.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidCapacity => "invalid-capacity",
            Self::Full => "full",
            Self::Poisoned => "poisoned",
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A fixed-capacity single-producer/single-consumer ring over caller-provided storage.
///
/// # Memory ordering
///
/// The producer writes a slot with `Relaxed` and then publishes the new `head` with `Release`; the
/// consumer loads `head` with `Acquire` (which orders every prior slot write), reads the slot, and
/// publishes the new `tail` with `Release`. The producer loads `tail` with `Acquire` to observe
/// the consumer's progress. This is the discipline `raven-ringbuf` already uses: free-running
/// `u64` counters masked for indexing, so the ring needs no sentinel slot and its capacity is
/// exactly the storage length.
#[derive(Debug)]
pub struct Ring<'a> {
    slots: &'a [AtomicU64],
    head: AtomicU64,
    tail: AtomicU64,
    poisoned: AtomicBool,
    /// Submissions rejected because the ring was full or poisoned.
    rejected: AtomicU64,
    mask: u64,
}

impl<'a> Ring<'a> {
    /// Borrow `slots` as the ring's storage. Its length must be a non-zero power of two.
    pub fn new(slots: &'a [AtomicU64]) -> Result<Self, Error> {
        if slots.is_empty() || !slots.len().is_power_of_two() {
            return Err(Error::InvalidCapacity);
        }
        Ok(Self {
            slots,
            head: AtomicU64::new(0),
            tail: AtomicU64::new(0),
            poisoned: AtomicBool::new(false),
            rejected: AtomicU64::new(0),
            mask: (slots.len() - 1) as u64,
        })
    }

    /// The fixed capacity (the storage length).
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// The number of descriptors currently queued.
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        head.wrapping_sub(tail) as usize
    }

    /// Whether no descriptor is queued.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether the ring is full.
    pub fn is_full(&self) -> bool {
        self.len() >= self.capacity()
    }

    /// Whether the ring has been poisoned.
    pub fn is_poisoned(&self) -> bool {
        self.poisoned.load(Ordering::Acquire)
    }

    /// Submissions rejected (full or poisoned) since the ring was created.
    pub fn rejected(&self) -> u64 {
        self.rejected.load(Ordering::Relaxed)
    }

    /// Submit one descriptor (producer). Fails with [`Error::Full`] when the ring is full and
    /// [`Error::Poisoned`] once poisoned.
    pub fn submit(&self, descriptor: Descriptor) -> Result<(), Error> {
        if self.poisoned.load(Ordering::Acquire) {
            return Err(Error::Poisoned);
        }
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= self.capacity() as u64 {
            self.rejected.fetch_add(1, Ordering::Relaxed);
            return Err(Error::Full);
        }
        let index = (head & self.mask) as usize;
        self.slots[index].store(descriptor.raw(), Ordering::Relaxed);
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// Submit a batch, stopping at the first rejection; returns the number accepted.
    pub fn submit_batch(&self, descriptors: &[Descriptor]) -> usize {
        let mut accepted = 0usize;
        for &descriptor in descriptors {
            if self.submit(descriptor).is_err() {
                break;
            }
            accepted = accepted.saturating_add(1);
        }
        accepted
    }

    /// Complete one descriptor (consumer), oldest first. Returns `None` when the ring is empty.
    ///
    /// A descriptor cancelled by [`Ring::cancel`] is returned with its cancelled bit set, so the
    /// consumer can report it without reordering the queue.
    pub fn complete(&self) -> Option<Descriptor> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let index = (tail & self.mask) as usize;
        let raw = self.slots[index].swap(0, Ordering::Relaxed);
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(Descriptor::from_raw(raw))
    }

    /// Complete up to `out.len()` descriptors; returns the number read.
    pub fn complete_batch(&self, out: &mut [Descriptor]) -> usize {
        let mut count = 0usize;
        for slot in out.iter_mut() {
            match self.complete() {
                Some(descriptor) => {
                    *slot = descriptor;
                    count = count.saturating_add(1);
                }
                None => break,
            }
        }
        count
    }

    /// Mark every pending descriptor with value `value` as cancelled (producer side); returns the
    /// number marked.
    ///
    /// Cancellation does not remove entries or reorder the queue: the consumer still pops the slot
    /// and observes the cancelled bit. Entries the consumer has already completed are unaffected.
    pub fn cancel(&self, value: u64) -> usize {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Relaxed);
        let mut cancelled = 0usize;
        let mut index = tail;
        while index != head {
            let slot = (index & self.mask) as usize;
            let raw = self.slots[slot].load(Ordering::Relaxed);
            let descriptor = Descriptor::from_raw(raw);
            if !descriptor.is_cancelled() && descriptor.value() == value {
                let new = Descriptor::cancelled(value).raw();
                if self.slots[slot]
                    .compare_exchange(raw, new, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    cancelled = cancelled.saturating_add(1);
                }
            }
            index = index.wrapping_add(1);
        }
        cancelled
    }

    /// Poison the ring: the terminal error state. Further submits fail with [`Error::Poisoned`];
    /// the consumer drains any remaining descriptors and then observes `None`.
    pub fn poison(&self) {
        self.poisoned.store(true, Ordering::Release);
    }
}

#[cfg(test)]
#[path = "ring_ut.rs"]
mod tests;
