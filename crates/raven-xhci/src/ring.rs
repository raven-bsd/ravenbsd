// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Command/transfer producer rings and the event-ring consumer.
//!
//! The rings live in one DMA buffer. Producers keep the last slot as a Link TRB with the current
//! cycle; when the producer reaches it, it writes the link, flips the cycle and continues at zero.
//! The event ring has no link: the controller writes events with its producer cycle and the host
//! consumes them with its own cycle.

use raven_rp1_xhci::Trb;

use crate::controller::Seam;
use crate::error::XhciError;

/// A producer ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProducerRing {
    offset: usize,
    entries: u16,
    enqueue: u16,
    cycle: bool,
}

impl ProducerRing {
    /// Build a ring with `entries` TRBs including the Link slot.
    ///
    /// # Errors
    ///
    /// Returns [`XhciError::Length`] for fewer than two or more than 1024 entries.
    pub const fn new(offset: usize, entries: u16) -> Result<Self, XhciError> {
        if entries < 2 || entries > 1024 {
            return Err(XhciError::Length);
        }
        Ok(Self {
            offset,
            entries,
            enqueue: 0,
            cycle: true,
        })
    }

    /// Write the initial Link TRB.
    ///
    /// # Errors
    ///
    /// Returns an [`XhciError`] when the DMA access fails.
    pub fn init<S: Seam>(&self, seam: &mut S) -> Result<(), XhciError> {
        let link = Trb::link(seam.dma_bus().saturating_add(self.offset as u64), true)
            .with_cycle(self.cycle);
        write_trb(seam, self.link_offset(), link)
    }

    /// Queue one TRB; returns its index.
    ///
    /// # Errors
    ///
    /// Returns an [`XhciError`] when the DMA access fails.
    pub fn push<S: Seam>(&mut self, seam: &mut S, trb: Trb) -> Result<u16, XhciError> {
        if self.enqueue >= self.entries.saturating_sub(1) {
            let link = Trb::link(seam.dma_bus().saturating_add(self.offset as u64), true)
                .with_cycle(self.cycle);
            write_trb(seam, self.link_offset(), link)?;
            self.enqueue = 0;
            self.cycle = !self.cycle;
        }
        let index = self.enqueue;
        let offset = self
            .offset
            .checked_add(usize::from(index).saturating_mul(16))
            .ok_or(XhciError::Length)?;
        write_trb(seam, offset, trb.with_cycle(self.cycle))?;
        self.enqueue = index.saturating_add(1);
        Ok(index)
    }

    /// The current producer index.
    #[must_use]
    pub const fn enqueue(&self) -> u16 {
        self.enqueue
    }

    /// The current producer cycle.
    #[must_use]
    pub const fn cycle(&self) -> bool {
        self.cycle
    }

    /// Ring slot count, including the Link slot.
    #[must_use]
    pub const fn entries(&self) -> u16 {
        self.entries
    }

    /// DMA offset of the ring.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    fn link_offset(&self) -> usize {
        self.offset
            .saturating_add(usize::from(self.entries.saturating_sub(1)).saturating_mul(16))
    }
}

/// The event-ring consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventRing {
    offset: usize,
    entries: u16,
    consumer: u16,
    cycle: bool,
}

impl EventRing {
    /// Build an event ring.
    ///
    /// # Errors
    ///
    /// Returns [`XhciError::Length`] for fewer than two or more than 1024 entries.
    pub const fn new(offset: usize, entries: u16) -> Result<Self, XhciError> {
        if entries < 2 || entries > 1024 {
            return Err(XhciError::Length);
        }
        Ok(Self {
            offset,
            entries,
            consumer: 0,
            cycle: true,
        })
    }

    /// Pop the next event, or `None` when the controller has not published one.
    ///
    /// # Errors
    ///
    /// Returns an [`XhciError`] when the DMA access fails.
    pub fn pop<S: Seam>(&mut self, seam: &S) -> Result<Option<Trb>, XhciError> {
        let offset = self
            .offset
            .checked_add(usize::from(self.consumer).saturating_mul(16))
            .ok_or(XhciError::Length)?;
        let trb = read_trb(seam, offset)?;
        if trb.cycle() != self.cycle {
            return Ok(None);
        }
        self.consumer = self.consumer.saturating_add(1);
        if self.consumer >= self.entries {
            self.consumer = 0;
            self.cycle = !self.cycle;
        }
        Ok(Some(trb))
    }

    /// DMA address of the next event for ERDP.
    #[must_use]
    pub fn dequeue_address(&self, bus: u64) -> u64 {
        bus.saturating_add(self.offset as u64)
            .saturating_add(u64::from(self.consumer).saturating_mul(16))
    }

    /// Current consumer index.
    #[must_use]
    pub const fn consumer(&self) -> u16 {
        self.consumer
    }

    /// Current consumer cycle.
    #[must_use]
    pub const fn cycle(&self) -> bool {
        self.cycle
    }
}

/// Write one 16-byte TRB into DMA.
pub fn write_trb<S: Seam>(seam: &mut S, offset: usize, trb: Trb) -> Result<(), XhciError> {
    seam.dma_write_u64(offset, trb.parameter)?;
    seam.dma_write_u32(offset.saturating_add(8), trb.status)?;
    seam.dma_write_u32(offset.saturating_add(12), trb.control)
}

/// Read one 16-byte TRB from DMA.
pub fn read_trb<S: Seam>(seam: &S, offset: usize) -> Result<Trb, XhciError> {
    Ok(Trb {
        parameter: seam.dma_read_u64(offset)?,
        status: seam.dma_read_u32(offset.saturating_add(8))?,
        control: seam.dma_read_u32(offset.saturating_add(12))?,
    })
}

#[cfg(test)]
#[path = "ring_ut.rs"]
mod tests;
