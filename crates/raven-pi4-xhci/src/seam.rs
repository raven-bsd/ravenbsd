// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The DMA, IRQ and transfer seam for S61 `raven-usb-core`.
//!
//! The model keeps the xHCI rings in [`XhciSeam`]; a platform implementation supplies the DMA
//! allocator, the doorbell write and the event wait. This is the integration surface the USB core
//! uses for enumeration and control transfers: push the Setup/Data/Status TRBs, ring the doorbell,
//! and wait for the matching Transfer Event. `no_std` callers provide their own `Host`.

use core::fmt;

use crate::ring::{CommandRing, EventRing, RingError, TransferRing};
use crate::trb::{CompletionCode, Trb, TrbType};

/// Maximum endpoint ID the seam accepts.
pub const MAX_ENDPOINT: u8 = 31;

/// Why a seam operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeamError {
    /// A ring operation failed.
    Ring(RingError),
    /// The endpoint ID is zero or above [`MAX_ENDPOINT`].
    Endpoint(u8),
    /// The transfer description had no TRBs.
    EmptyTransfer,
    /// The host supplied no event before the timeout.
    Timeout,
    /// The host event did not complete the requested transfer.
    UnexpectedEvent,
}

impl fmt::Display for SeamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ring(error) => write!(f, "ring: {error}"),
            Self::Endpoint(endpoint) => {
                write!(f, "endpoint {endpoint} is outside 1..={MAX_ENDPOINT}")
            }
            Self::EmptyTransfer => f.write_str("transfer has no TRBs"),
            Self::Timeout => f.write_str("transfer timed out"),
            Self::UnexpectedEvent => f.write_str("unexpected event"),
        }
    }
}

impl core::error::Error for SeamError {}

impl From<RingError> for SeamError {
    fn from(error: RingError) -> Self {
        Self::Ring(error)
    }
}

/// DMA direction for a transfer buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaDirection {
    /// Host-to-device.
    Out,
    /// Device-to-host.
    In,
}

/// One DMA buffer as seen by the controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaBuffer {
    /// CPU virtual address used by the driver.
    pub cpu: usize,
    /// Bus address the controller uses.
    pub bus: u64,
    /// Buffer length in bytes.
    pub len: usize,
}

/// A completed transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completion {
    /// Slot ID.
    pub slot: u8,
    /// Endpoint ID.
    pub endpoint: u8,
    /// Completion code.
    pub code: CompletionCode,
    /// Residual length in bytes.
    pub length: u32,
}

/// One transfer description.
#[derive(Clone, Copy, Debug)]
pub struct Transfer<'a> {
    /// Slot ID.
    pub slot: u8,
    /// Endpoint ID.
    pub endpoint: u8,
    /// Setup/Data/Status TRBs in order.
    pub trbs: &'a [Trb],
}

/// The platform hooks the seam needs.
pub trait Host {
    /// Allocate a DMA buffer of `len` bytes with at least `align` alignment.
    fn allocate_dma(&mut self, len: usize, align: usize) -> Option<DmaBuffer>;
    /// Release a buffer returned by [`Host::allocate_dma`].
    fn free_dma(&mut self, buffer: DmaBuffer);
    /// Ring the doorbell for `slot` and endpoint `target`.
    fn doorbell(&mut self, slot: u8, target: u8);
    /// Wait for one event; the platform inserts it into `events` and returns it.
    fn wait_event(&mut self, events: &mut EventRing, timeout_ns: u64) -> Option<Trb>;
    /// Acknowledge the controller interrupt.
    fn acknowledge_irq(&mut self);
}

/// The xHCI seam: command/transfer rings, event ring and a platform host.
#[derive(Clone, Debug)]
pub struct XhciSeam<H> {
    host: H,
    commands: CommandRing,
    transfers: TransferRing,
    events: EventRing,
}

impl<H: Host> XhciSeam<H> {
    /// Build a seam with 16-entry rings.
    ///
    /// # Errors
    ///
    /// Returns [`RingError`] when a ring size is rejected.
    pub fn new(host: H) -> Result<Self, RingError> {
        Ok(Self {
            host,
            commands: CommandRing::new(16)?,
            transfers: TransferRing::new(16)?,
            events: EventRing::new(16)?,
        })
    }

    /// The platform host.
    #[must_use]
    pub fn host(&self) -> &H {
        &self.host
    }

    /// The platform host, mutably.
    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }

    /// The command ring.
    #[must_use]
    pub fn command_ring(&self) -> &CommandRing {
        &self.commands
    }

    /// The command ring, mutably.
    pub fn command_ring_mut(&mut self) -> &mut CommandRing {
        &mut self.commands
    }

    /// The transfer ring.
    #[must_use]
    pub fn transfer_ring(&self) -> &TransferRing {
        &self.transfers
    }

    /// The transfer ring, mutably.
    pub fn transfer_ring_mut(&mut self) -> &mut TransferRing {
        &mut self.transfers
    }

    /// The event ring.
    #[must_use]
    pub fn event_ring(&self) -> &EventRing {
        &self.events
    }

    /// The event ring, mutably.
    pub fn event_ring_mut(&mut self) -> &mut EventRing {
        &mut self.events
    }

    /// Allocate DMA through the platform.
    pub fn allocate(&mut self, len: usize, align: usize) -> Option<DmaBuffer> {
        self.host.allocate_dma(len, align)
    }

    /// Release DMA through the platform.
    pub fn free(&mut self, buffer: DmaBuffer) {
        self.host.free_dma(buffer);
    }

    /// Submit one Setup/Data/Status sequence and wait for its Transfer Event.
    ///
    /// # Errors
    ///
    /// Returns an endpoint/empty-transfer/ring error before ringing the doorbell, or
    /// [`SeamError::Timeout`] when the platform supplies no matching event.
    pub fn submit(
        &mut self,
        transfer: Transfer<'_>,
        timeout_ns: u64,
    ) -> Result<Completion, SeamError> {
        if transfer.endpoint == 0 || transfer.endpoint > MAX_ENDPOINT {
            return Err(SeamError::Endpoint(transfer.endpoint));
        }
        if transfer.trbs.is_empty() {
            return Err(SeamError::EmptyTransfer);
        }
        let usable = usize::from(self.transfers.capacity().saturating_sub(1));
        if usize::from(self.transfers.pending()).saturating_add(transfer.trbs.len()) > usable {
            return Err(SeamError::Ring(RingError::Full));
        }
        for trb in transfer.trbs {
            self.transfers.push(*trb)?;
        }
        self.host.doorbell(transfer.slot, transfer.endpoint);
        let mut attempts = 0u8;
        while attempts < 16 {
            attempts = attempts.saturating_add(1);
            let Some(event) = self.host.wait_event(&mut self.events, timeout_ns) else {
                return Err(SeamError::Timeout);
            };
            if event.kind() != TrbType::TransferEvent {
                continue;
            }
            if event.slot() != transfer.slot || event.endpoint() != transfer.endpoint {
                return Err(SeamError::UnexpectedEvent);
            }
            for _ in 0..transfer.trbs.len() {
                self.transfers.complete();
            }
            self.host.acknowledge_irq();
            return Ok(Completion {
                slot: event.slot(),
                endpoint: event.endpoint(),
                code: event.completion(),
                length: event.length(),
            });
        }
        Err(SeamError::Timeout)
    }
}

#[cfg(test)]
#[path = "seam_ut.rs"]
mod tests;
