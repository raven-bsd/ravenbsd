// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The transfer-ring seam over the R4 xHCI model.
//!
//! A control transfer becomes a Setup/Data/Status TRB sequence. The seam stamps the endpoint and
//! slot, sets the chain bit on every stage except the final Status Stage, pushes the sequence onto
//! an [`raven_rp1_xhci::TransferRing`] and turns controller events from an
//! [`raven_rp1_xhci::EventRing`] into typed completions. This is a host-side model: no register is
//! touched and the real controller's DMA/interrupt path remains UNVERIFIED.

use raven_rp1_xhci::{
    ControlTransfer, EventRing, RingError, TRB_CHAIN, TransferRing, Trb, TrbType,
};

use crate::error::UsbError;

/// xHCI completion code for a successful transfer.
pub const COMPLETION_SUCCESS: u8 = 1;
/// xHCI completion code for a STALL.
pub const COMPLETION_STALL: u8 = 6;

/// A completed transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completion {
    /// Slot ID.
    pub slot: u8,
    /// Endpoint DCI.
    pub endpoint: u8,
    /// xHCI completion code.
    pub completion_code: u8,
    /// TRB pointer reported by the controller.
    pub trb_pointer: u64,
}

impl Completion {
    /// True for the success completion code.
    #[must_use]
    pub const fn success(self) -> bool {
        self.completion_code == COMPLETION_SUCCESS
    }

    /// True for a STALL completion code.
    #[must_use]
    pub const fn stalled(self) -> bool {
        self.completion_code == COMPLETION_STALL
    }
}

/// A command completion event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandCompletion {
    /// Slot ID.
    pub slot: u8,
    /// xHCI completion code.
    pub completion_code: u8,
    /// Command TRB pointer.
    pub command_pointer: u64,
}

/// One event dequeued from the controller event ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XhciEvent {
    /// A transfer completion.
    Transfer(Completion),
    /// A command completion.
    Command(CommandCompletion),
    /// A port-status-change event for `port`.
    Port(u8),
    /// Any other event type.
    Other(TrbType),
}

/// The xHCI transfer-ring seam.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XhciSeam {
    ring: TransferRing,
    events: EventRing,
    slot: u8,
    endpoint: u8,
}

impl XhciSeam {
    /// Build a seam for `slot` and endpoint DCI `endpoint`.
    ///
    /// # Errors
    ///
    /// Returns [`UsbError::Ring`] for an invalid ring size, or a state error for slot 0 or an
    /// endpoint outside `1..=31`.
    pub fn new(
        ring_entries: u16,
        event_entries: u16,
        slot: u8,
        endpoint: u8,
    ) -> Result<Self, UsbError> {
        if slot == 0 {
            return Err(UsbError::State {
                expected: "a non-zero slot",
                actual: "slot 0",
            });
        }
        if endpoint == 0 || endpoint > 31 {
            return Err(UsbError::State {
                expected: "endpoint 1..=31",
                actual: "invalid endpoint",
            });
        }
        Ok(Self {
            ring: TransferRing::new(ring_entries)?,
            events: EventRing::new(event_entries)?,
            slot,
            endpoint,
        })
    }

    /// Slot ID.
    #[must_use]
    pub const fn slot(&self) -> u8 {
        self.slot
    }

    /// Endpoint DCI.
    #[must_use]
    pub const fn endpoint(&self) -> u8 {
        self.endpoint
    }

    /// Queue one control transfer; returns the number of TRBs queued.
    ///
    /// # Errors
    ///
    /// Returns [`UsbError::Ring`] when the ring cannot accept the whole sequence.
    pub fn queue(&mut self, transfer: &ControlTransfer) -> Result<u16, UsbError> {
        let (trbs, count) = transfer.trbs();
        if self.ring.free_slots() < u16::from(count) {
            return Err(UsbError::Ring(RingError::Full));
        }
        for (index, trb) in trbs.iter().take(usize::from(count)).enumerate() {
            let mut trb = *trb;
            if index + 1 < usize::from(count) {
                trb.control |= TRB_CHAIN;
            }
            let trb = trb
                .with_endpoint(self.endpoint)
                .ok_or(UsbError::State {
                    expected: "endpoint 1..=31",
                    actual: "invalid endpoint",
                })?
                .with_slot(self.slot);
            self.ring.push(trb)?;
        }
        Ok(u16::from(count))
    }

    /// Publish a controller event.
    ///
    /// # Errors
    ///
    /// Returns [`UsbError::Ring`] when the event ring is full.
    pub fn push_event(&mut self, event: Trb) -> Result<u16, UsbError> {
        Ok(self.events.push_event(event)?)
    }

    /// Consume one controller event.
    pub fn poll(&mut self) -> Option<XhciEvent> {
        let (_, event) = self.events.pop()?;
        match event.kind() {
            TrbType::TransferEvent => Some(XhciEvent::Transfer(Completion {
                slot: event.slot(),
                endpoint: event.endpoint(),
                completion_code: event.completion_code(),
                trb_pointer: event.parameter,
            })),
            TrbType::CommandCompletion => Some(XhciEvent::Command(CommandCompletion {
                slot: event.slot(),
                completion_code: event.completion_code(),
                command_pointer: event.parameter,
            })),
            TrbType::PortStatusChange => Some(XhciEvent::Port((event.parameter >> 24) as u8)),
            other => Some(XhciEvent::Other(other)),
        }
    }

    /// Mark the oldest transfer complete in the ring.
    pub fn complete_transfer(&mut self) -> Option<u16> {
        self.ring.complete()
    }

    /// Transfers in flight.
    #[must_use]
    pub const fn pending(&self) -> u16 {
        self.ring.pending()
    }

    /// Free transfer slots.
    #[must_use]
    pub const fn free_slots(&self) -> u16 {
        self.ring.free_slots()
    }

    /// The transfer ring.
    #[must_use]
    pub const fn ring(&self) -> &TransferRing {
        &self.ring
    }

    /// The event ring.
    #[must_use]
    pub const fn events(&self) -> &EventRing {
        &self.events
    }
}

#[cfg(test)]
#[path = "seam_ut.rs"]
mod tests;
