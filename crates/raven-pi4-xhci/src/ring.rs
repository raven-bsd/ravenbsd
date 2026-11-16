// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The xHCI command, event and transfer rings.
//!
//! Command and transfer rings reserve the last slot for a Link TRB; when the producer reaches it,
//! the ring writes the link and continues at index zero with the next cycle. The event ring is
//! written by the controller and consumed by the host with its own cycle bit; a full event ring
//! reports `Full` instead of overwriting an unconsumed event.

use core::fmt;

use crate::trb::Trb;

/// Largest ring the model holds.
pub const MAX_RING_ENTRIES: u16 = 64;

/// Why a ring operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingError {
    /// A ring needs at least two entries.
    TooSmall(u16),
    /// The requested size exceeds [`MAX_RING_ENTRIES`].
    TooManyEntries(u16),
    /// The requested size is not a power of two.
    NotPowerOfTwo(u16),
    /// Every usable slot is in flight.
    Full,
}

impl fmt::Display for RingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooSmall(entries) => write!(f, "ring of {entries} entries is too small"),
            Self::TooManyEntries(entries) => {
                write!(f, "ring of {entries} entries exceeds {MAX_RING_ENTRIES}")
            }
            Self::NotPowerOfTwo(entries) => {
                write!(f, "ring of {entries} entries is not a power of two")
            }
            Self::Full => f.write_str("ring is full"),
        }
    }
}

impl core::error::Error for RingError {}

const EMPTY_TRB: Trb = Trb {
    parameter: 0,
    status: 0,
    control: 0,
};

const fn validate(entries: u16) -> Result<(), RingError> {
    if entries < 2 {
        return Err(RingError::TooSmall(entries));
    }
    if entries > MAX_RING_ENTRIES {
        return Err(RingError::TooManyEntries(entries));
    }
    if entries & entries.wrapping_sub(1) != 0 {
        return Err(RingError::NotPowerOfTwo(entries));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Producer {
    entries: [Trb; MAX_RING_ENTRIES as usize],
    count: u16,
    enqueue: u16,
    dequeue: u16,
    cycle: bool,
    used: u16,
}

impl Producer {
    const fn new(entries: u16) -> Result<Self, RingError> {
        match validate(entries) {
            Ok(()) => Ok(Self {
                entries: [EMPTY_TRB; MAX_RING_ENTRIES as usize],
                count: entries,
                enqueue: 0,
                dequeue: 0,
                cycle: true,
                used: 0,
            }),
            Err(error) => Err(error),
        }
    }

    const fn usable(&self) -> u16 {
        self.count.saturating_sub(1)
    }

    fn push(&mut self, trb: Trb) -> Result<u16, RingError> {
        if self.used >= self.usable() {
            return Err(RingError::Full);
        }
        let index = self.enqueue;
        let Some(slot) = self.entries.get_mut(usize::from(index)) else {
            return Err(RingError::Full);
        };
        *slot = trb.with_cycle(self.cycle);
        self.enqueue = index.saturating_add(1);
        if self.enqueue >= self.usable() {
            let link_index = self.count.saturating_sub(1);
            if let Some(link) = self.entries.get_mut(usize::from(link_index)) {
                *link = Trb::link(0, true).with_cycle(self.cycle);
            }
            self.enqueue = 0;
            self.cycle = !self.cycle;
        }
        self.used = self.used.saturating_add(1);
        Ok(index)
    }

    fn complete(&mut self) -> Option<u16> {
        if self.used == 0 {
            return None;
        }
        let index = self.dequeue;
        self.dequeue = index.saturating_add(1);
        if self.dequeue >= self.usable() {
            self.dequeue = 0;
        }
        self.used = self.used.saturating_sub(1);
        Some(index)
    }

    fn slot(&self, index: u16) -> Option<&Trb> {
        if index >= self.count {
            return None;
        }
        self.entries.get(index as usize)
    }
}

/// The command ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandRing {
    inner: Producer,
}

impl CommandRing {
    /// Build a command ring.
    ///
    /// # Errors
    ///
    /// Returns [`RingError`] for a size below 2, above [`MAX_RING_ENTRIES`] or not a power of two.
    pub const fn new(entries: u16) -> Result<Self, RingError> {
        match Producer::new(entries) {
            Ok(inner) => Ok(Self { inner }),
            Err(error) => Err(error),
        }
    }

    /// Queue a command; returns its slot index.
    ///
    /// # Errors
    ///
    /// Returns [`RingError::Full`] when every usable slot is in flight.
    pub fn push(&mut self, trb: Trb) -> Result<u16, RingError> {
        self.inner.push(trb)
    }

    /// Mark the oldest command complete; returns its slot index.
    pub fn complete(&mut self) -> Option<u16> {
        self.inner.complete()
    }

    /// Commands in flight.
    #[must_use]
    pub const fn pending(&self) -> u16 {
        self.inner.used
    }

    /// Next command slot.
    #[must_use]
    pub const fn enqueue(&self) -> u16 {
        self.inner.enqueue
    }

    /// Current producer cycle.
    #[must_use]
    pub const fn cycle(&self) -> bool {
        self.inner.cycle
    }

    /// The TRB at `index`, when it is inside the array.
    #[must_use]
    pub fn slot(&self, index: u16) -> Option<&Trb> {
        self.inner.slot(index)
    }
}

/// One endpoint's transfer ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferRing {
    inner: Producer,
}

impl TransferRing {
    /// Build a transfer ring.
    ///
    /// # Errors
    ///
    /// Returns [`RingError`] for a size below 2, above [`MAX_RING_ENTRIES`] or not a power of two.
    pub const fn new(entries: u16) -> Result<Self, RingError> {
        match Producer::new(entries) {
            Ok(inner) => Ok(Self { inner }),
            Err(error) => Err(error),
        }
    }

    /// Queue a transfer TRB; returns its slot index.
    ///
    /// # Errors
    ///
    /// Returns [`RingError::Full`] when every usable slot is in flight.
    pub fn push(&mut self, trb: Trb) -> Result<u16, RingError> {
        self.inner.push(trb)
    }

    /// Mark the oldest transfer complete; returns its slot index.
    pub fn complete(&mut self) -> Option<u16> {
        self.inner.complete()
    }

    /// Transfers in flight.
    #[must_use]
    pub const fn pending(&self) -> u16 {
        self.inner.used
    }

    /// Total ring capacity, including the Link slot.
    #[must_use]
    pub const fn capacity(&self) -> u16 {
        self.inner.count
    }

    /// The TRB at `index`, when it is inside the array.
    #[must_use]
    pub fn slot(&self, index: u16) -> Option<&Trb> {
        self.inner.slot(index)
    }
}

/// The event ring written by the controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventRing {
    entries: [Trb; MAX_RING_ENTRIES as usize],
    count: u16,
    enqueue: u16,
    dequeue: u16,
    producer_cycle: bool,
    used: u16,
}

impl EventRing {
    /// Build an event ring.
    ///
    /// # Errors
    ///
    /// Returns [`RingError`] for a size below 2, above [`MAX_RING_ENTRIES`] or not a power of two.
    pub const fn new(entries: u16) -> Result<Self, RingError> {
        match validate(entries) {
            Ok(()) => Ok(Self {
                entries: [EMPTY_TRB; MAX_RING_ENTRIES as usize],
                count: entries,
                enqueue: 0,
                dequeue: 0,
                producer_cycle: true,
                used: 0,
            }),
            Err(error) => Err(error),
        }
    }

    /// Post an event; returns its slot index.
    ///
    /// # Errors
    ///
    /// Returns [`RingError::Full`] when the consumer has not drained the ring.
    pub fn push(&mut self, trb: Trb) -> Result<u16, RingError> {
        if self.used >= self.count {
            return Err(RingError::Full);
        }
        let index = self.enqueue;
        let Some(slot) = self.entries.get_mut(usize::from(index)) else {
            return Err(RingError::Full);
        };
        *slot = trb.with_cycle(self.producer_cycle);
        self.enqueue = (index + 1) % self.count;
        if self.enqueue == 0 {
            self.producer_cycle = !self.producer_cycle;
        }
        self.used = self.used.saturating_add(1);
        Ok(index)
    }

    /// Consume the oldest event.
    pub fn pop(&mut self) -> Option<Trb> {
        if self.used == 0 {
            return None;
        }
        let index = self.dequeue;
        let trb = self.entries.get(usize::from(index)).copied();
        self.dequeue = (index + 1) % self.count;
        self.used = self.used.saturating_sub(1);
        trb
    }

    /// Events waiting to be consumed.
    #[must_use]
    pub const fn pending(&self) -> u16 {
        self.used
    }

    /// Next producer slot.
    #[must_use]
    pub const fn enqueue(&self) -> u16 {
        self.enqueue
    }

    /// Current producer cycle.
    #[must_use]
    pub const fn cycle(&self) -> bool {
        self.producer_cycle
    }

    /// The event at `index`, when it is inside the array.
    #[must_use]
    pub fn slot(&self, index: u16) -> Option<&Trb> {
        if index >= self.count {
            return None;
        }
        self.entries.get(index as usize)
    }
}

#[cfg(test)]
#[path = "ring_ut.rs"]
mod tests;
