// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RX/TX frame paths with per-slot ownership, refill and ring wrap.
//!
//! The hardware descriptor is MMIO, but the driver still needs to know which slot is free, which
//! one the device owns, and which completed slot must be refilled. [`RxPath`] and [`TxPath`]
//! keep that state next to [`RingIndices`], validate every completed status through the
//! descriptor model, and free a malformed slot so one bad frame cannot stall the ring.

use crate::regs::DESC_COUNT;
use crate::ring::{
    BUFFER_LEN, RingError, RingIndices, RxDescriptor, RxError, RxFrame, RxStatus, TxDescriptor,
    TxError, TxStatus,
};

/// Number of comparison slots in the fixed ownership table.
pub const SLOT_COUNT: usize = DESC_COUNT as usize;
/// Packet buffers must be aligned for the non-coherent cache line.
pub const FRAME_ALIGN: u64 = 64;

/// Who currently owns one descriptor slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotOwner {
    /// The driver may refill the slot.
    Free,
    /// The CPU is filling or has completed the slot.
    Driver,
    /// The device owns the slot.
    Device,
}

/// One RX ownership slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxSlot {
    /// Current owner.
    pub owner: SlotOwner,
    /// Last descriptor written for the slot.
    pub descriptor: RxDescriptor,
}

/// One TX ownership slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxSlot {
    /// Current owner.
    pub owner: SlotOwner,
    /// Last descriptor written for the slot.
    pub descriptor: TxDescriptor,
}

/// Why a frame-path operation was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// The ring index model rejected the operation.
    Ring(RingError),
    /// A packet-buffer address is zero or not [`FRAME_ALIGN`] aligned.
    Address(u64),
    /// The frame length is zero or exceeds one buffer.
    Length(usize),
    /// The ring has no free descriptor.
    NoFreeSlot,
    /// The slot is not in the free state.
    SlotBusy(u16),
    /// The completion names a slot the device does not own.
    SlotNotDevice(u16),
    /// The device still owns the descriptor.
    StillOwned(u16),
    /// The status could not be encoded as a TX descriptor.
    BadStatus(TxError),
}

impl From<RingError> for FrameError {
    fn from(error: RingError) -> Self {
        Self::Ring(error)
    }
}

/// A posted RX descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxPost {
    /// Descriptor slot index.
    pub index: u16,
    /// Descriptor to write to the device.
    pub descriptor: RxDescriptor,
}

/// A completed RX slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxCompletion {
    /// Descriptor slot index.
    pub index: u16,
    /// Completed descriptor.
    pub descriptor: RxDescriptor,
    /// Validated frame, or the malformed-status reason.
    pub frame: Result<RxFrame, RxError>,
}

/// A queued TX descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxPost {
    /// Descriptor slot index.
    pub index: u16,
    /// Descriptor to write to the device.
    pub descriptor: TxDescriptor,
}

/// A reclaimed TX slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxCompletion {
    /// Descriptor slot index.
    pub index: u16,
    /// Completed descriptor.
    pub descriptor: TxDescriptor,
    /// Completed status.
    pub status: TxStatus,
}

const EMPTY_RX_SLOT: RxSlot = RxSlot {
    owner: SlotOwner::Free,
    descriptor: RxDescriptor::new(0, RxStatus::from_raw(0)),
};

const EMPTY_TX_SLOT: TxSlot = TxSlot {
    owner: SlotOwner::Free,
    descriptor: TxDescriptor::new(0, TxStatus::from_raw(0)),
};

/// The RX ownership table and its ring indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RxPath {
    indices: RingIndices,
    slots: [RxSlot; SLOT_COUNT],
}

impl RxPath {
    /// Create an empty RX path with `count` slots.
    pub fn new(count: u16) -> Result<Self, FrameError> {
        if count == 0 {
            return Err(FrameError::Ring(RingError::ZeroCount));
        }
        if count > DESC_COUNT {
            return Err(FrameError::Ring(RingError::TooManyEntries));
        }
        Ok(Self {
            indices: RingIndices::new(count)?,
            slots: [EMPTY_RX_SLOT; SLOT_COUNT],
        })
    }

    /// The ring indices.
    #[must_use]
    pub const fn indices(&self) -> RingIndices {
        self.indices
    }

    /// The number of slots.
    #[must_use]
    pub const fn count(&self) -> u16 {
        self.indices.count()
    }

    /// The owner of one slot, or `None` for an out-of-range index.
    #[must_use]
    pub fn slot_owner(&self, index: u16) -> Option<SlotOwner> {
        if index >= self.indices.count() {
            return None;
        }
        self.slots.get(index as usize).map(|slot| slot.owner)
    }

    /// The descriptor stored in one slot.
    #[must_use]
    pub fn descriptor(&self, index: u16) -> Option<RxDescriptor> {
        if index >= self.indices.count() {
            return None;
        }
        self.slots.get(index as usize).map(|slot| slot.descriptor)
    }

    /// Post the next free RX slot to the device.
    pub fn post_next(&mut self, address: u64) -> Result<RxPost, FrameError> {
        validate_address(address)?;
        if self.indices.free()? == 0 {
            return Err(FrameError::NoFreeSlot);
        }
        let slot = self.indices.producer_slot();
        let entry = self.slots.get_mut(slot).ok_or(FrameError::NoFreeSlot)?;
        if entry.owner != SlotOwner::Free {
            return Err(FrameError::SlotBusy(slot as u16));
        }
        let descriptor = RxDescriptor::new(address, RxStatus::from_raw(0));
        entry.owner = SlotOwner::Device;
        entry.descriptor = descriptor;
        self.indices.advance_producer(1)?;
        Ok(RxPost {
            index: slot as u16,
            descriptor,
        })
    }

    /// Refill the next free RX slot; the operation is the same as [`Self::post_next`].
    pub fn refill_next(&mut self, address: u64) -> Result<RxPost, FrameError> {
        self.post_next(address)
    }

    /// Complete the next device-owned RX slot.
    ///
    /// A malformed status still frees and advances the slot, returning the reason in
    /// [`RxCompletion::frame`]. A still-owned descriptor is not advanced.
    pub fn complete_next(&mut self, status: RxStatus) -> Result<Option<RxCompletion>, FrameError> {
        if self.indices.used()? == 0 {
            return Ok(None);
        }
        let slot = self.indices.consumer_slot();
        let entry = self
            .slots
            .get_mut(slot)
            .ok_or(FrameError::SlotNotDevice(slot as u16))?;
        if entry.owner != SlotOwner::Device {
            return Err(FrameError::SlotNotDevice(slot as u16));
        }
        if status.own() {
            return Err(FrameError::StillOwned(slot as u16));
        }
        let descriptor = RxDescriptor::new(entry.descriptor.address, status);
        let frame = status.frame();
        entry.owner = SlotOwner::Free;
        entry.descriptor = descriptor;
        self.indices.advance_consumer(1)?;
        Ok(Some(RxCompletion {
            index: slot as u16,
            descriptor,
            frame,
        }))
    }

    /// Number of descriptors currently owned by the device.
    pub fn posted(&self) -> Result<usize, FrameError> {
        Ok(self.indices.used()?)
    }

    /// Number of free descriptor slots.
    pub fn free_slots(&self) -> Result<usize, FrameError> {
        Ok(self.indices.free()?)
    }
}

/// The TX ownership table and its ring indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxPath {
    indices: RingIndices,
    slots: [TxSlot; SLOT_COUNT],
}

impl TxPath {
    /// Create an empty TX path with `count` slots.
    pub fn new(count: u16) -> Result<Self, FrameError> {
        if count == 0 {
            return Err(FrameError::Ring(RingError::ZeroCount));
        }
        if count > DESC_COUNT {
            return Err(FrameError::Ring(RingError::TooManyEntries));
        }
        Ok(Self {
            indices: RingIndices::new(count)?,
            slots: [EMPTY_TX_SLOT; SLOT_COUNT],
        })
    }

    /// The ring indices.
    #[must_use]
    pub const fn indices(&self) -> RingIndices {
        self.indices
    }

    /// The number of slots.
    #[must_use]
    pub const fn count(&self) -> u16 {
        self.indices.count()
    }

    /// The owner of one slot, or `None` for an out-of-range index.
    #[must_use]
    pub fn slot_owner(&self, index: u16) -> Option<SlotOwner> {
        if index >= self.indices.count() {
            return None;
        }
        self.slots.get(index as usize).map(|slot| slot.owner)
    }

    /// The descriptor stored in one slot.
    #[must_use]
    pub fn descriptor(&self, index: u16) -> Option<TxDescriptor> {
        if index >= self.indices.count() {
            return None;
        }
        self.slots.get(index as usize).map(|slot| slot.descriptor)
    }

    /// Queue the next free TX slot for the device.
    pub fn queue_next(&mut self, address: u64, length: usize) -> Result<TxPost, FrameError> {
        validate_address(address)?;
        if length == 0 || length > BUFFER_LEN {
            return Err(FrameError::Length(length));
        }
        if self.indices.free()? == 0 {
            return Err(FrameError::NoFreeSlot);
        }
        let slot = self.indices.producer_slot();
        let entry = self.slots.get_mut(slot).ok_or(FrameError::NoFreeSlot)?;
        if entry.owner != SlotOwner::Free {
            return Err(FrameError::SlotBusy(slot as u16));
        }
        let status = TxStatus::single(length).map_err(FrameError::BadStatus)?;
        let descriptor = TxDescriptor::new(address, status);
        entry.owner = SlotOwner::Device;
        entry.descriptor = descriptor;
        self.indices.advance_producer(1)?;
        Ok(TxPost {
            index: slot as u16,
            descriptor,
        })
    }

    /// Reclaim the next device-owned TX slot.
    ///
    /// A still-owned status is not advanced; a completed status frees and advances the slot.
    pub fn reclaim_next(&mut self, status: TxStatus) -> Result<Option<TxCompletion>, FrameError> {
        if self.indices.used()? == 0 {
            return Ok(None);
        }
        let slot = self.indices.consumer_slot();
        let entry = self
            .slots
            .get_mut(slot)
            .ok_or(FrameError::SlotNotDevice(slot as u16))?;
        if entry.owner != SlotOwner::Device {
            return Err(FrameError::SlotNotDevice(slot as u16));
        }
        if status.own() {
            return Err(FrameError::StillOwned(slot as u16));
        }
        let descriptor = TxDescriptor::new(entry.descriptor.address, status);
        entry.owner = SlotOwner::Free;
        entry.descriptor = descriptor;
        self.indices.advance_consumer(1)?;
        Ok(Some(TxCompletion {
            index: slot as u16,
            descriptor,
            status,
        }))
    }

    /// Number of descriptors currently owned by the device.
    pub fn posted(&self) -> Result<usize, FrameError> {
        Ok(self.indices.used()?)
    }

    /// Number of free descriptor slots.
    pub fn free_slots(&self) -> Result<usize, FrameError> {
        Ok(self.indices.free()?)
    }
}

/// The paired RX/TX frame path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FramePath {
    /// Receive path.
    pub rx: RxPath,
    /// Transmit path.
    pub tx: TxPath,
}

impl FramePath {
    /// Create both paths with the same slot count.
    pub fn new(count: u16) -> Result<Self, FrameError> {
        Ok(Self {
            rx: RxPath::new(count)?,
            tx: TxPath::new(count)?,
        })
    }
}

fn validate_address(address: u64) -> Result<(), FrameError> {
    if address == 0 || !address.is_multiple_of(FRAME_ALIGN) {
        return Err(FrameError::Address(address));
    }
    Ok(())
}

#[cfg(test)]
#[path = "frame_ut.rs"]
mod tests;
