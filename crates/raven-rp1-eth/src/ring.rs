// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TX/RX descriptor rings.
//!
//! A ring is a fixed power-of-two set of descriptors with a host/device ownership bit. The producer
//! submits descriptors at the tail, the device consumes them from the head, and the host reclaims
//! completed descriptors in FIFO order. The register and in-memory descriptor encoding of the RP1
//! GEM is UNVERIFIED, so this is a typed model; the board bring-up maps it onto the real 32/64-bit
//! descriptor words.

use core::fmt;

/// Largest number of descriptors the model holds.
pub const MAX_ENTRIES: u16 = 256;

/// Who may touch a descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// The host owns the slot.
    Host,
    /// The device owns the slot and is expected to complete it.
    Device,
}

/// The descriptor flags the model tracks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DescFlags {
    /// First descriptor of a frame.
    pub first: bool,
    /// Last descriptor of a frame.
    pub last: bool,
    /// The device reported an error for this descriptor.
    pub error: bool,
}

/// One descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Descriptor {
    /// DMA buffer address.
    pub buffer: u64,
    /// Buffer length in bytes.
    pub length: u16,
    /// Frame flags.
    pub flags: DescFlags,
    /// Current owner.
    pub owner: Owner,
}

impl Descriptor {
    /// A host-owned descriptor with no flags.
    #[must_use]
    pub const fn new(buffer: u64, length: u16) -> Self {
        Self {
            buffer,
            length,
            flags: DescFlags {
                first: true,
                last: true,
                error: false,
            },
            owner: Owner::Host,
        }
    }

    /// Set the frame flags.
    #[must_use]
    pub const fn with_flags(self, flags: DescFlags) -> Self {
        Self { flags, ..self }
    }

    /// A zeroed slot used as an array initialiser.
    const EMPTY: Self = Self {
        buffer: 0,
        length: 0,
        flags: DescFlags {
            first: false,
            last: false,
            error: false,
        },
        owner: Owner::Host,
    };
}

/// Why a ring operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingError {
    /// The requested capacity is zero.
    ZeroEntries,
    /// The requested capacity exceeds [`MAX_ENTRIES`].
    TooManyEntries(u16),
    /// The requested capacity is not a power of two.
    NotPowerOfTwo(u16),
    /// No free slot.
    Full,
}

impl fmt::Display for RingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroEntries => f.write_str("descriptor ring needs at least one entry"),
            Self::TooManyEntries(entries) => {
                write!(f, "descriptor ring of {entries} exceeds {MAX_ENTRIES}")
            }
            Self::NotPowerOfTwo(entries) => {
                write!(f, "descriptor ring of {entries} is not a power of two")
            }
            Self::Full => f.write_str("descriptor ring is full"),
        }
    }
}

impl core::error::Error for RingError {}

/// One descriptor ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ring {
    descriptors: [Descriptor; MAX_ENTRIES as usize],
    count: u16,
    head: u16,
    used: u16,
}

impl Ring {
    /// Build a ring.
    ///
    /// # Errors
    ///
    /// Returns [`RingError`] for zero entries, too many entries or a non-power-of-two capacity.
    pub const fn new(entries: u16) -> Result<Self, RingError> {
        if entries == 0 {
            return Err(RingError::ZeroEntries);
        }
        if entries > MAX_ENTRIES {
            return Err(RingError::TooManyEntries(entries));
        }
        if entries & entries.wrapping_sub(1) != 0 {
            return Err(RingError::NotPowerOfTwo(entries));
        }
        Ok(Self {
            descriptors: [Descriptor::EMPTY; MAX_ENTRIES as usize],
            count: entries,
            head: 0,
            used: 0,
        })
    }

    /// Capacity in descriptors.
    #[must_use]
    pub const fn capacity(&self) -> u16 {
        self.count
    }

    /// Descriptors submitted but not completed.
    #[must_use]
    pub const fn len(&self) -> u16 {
        self.used
    }

    /// True when no descriptor is in flight.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.used == 0
    }

    /// True when every slot is in flight.
    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.used == self.count
    }

    /// The index the next submission uses.
    #[must_use]
    pub const fn tail(&self) -> u16 {
        (self.head + self.used) % self.count
    }

    /// The descriptor at `index`, or `None` outside the ring.
    #[must_use]
    pub fn descriptor(&self, index: u16) -> Option<&Descriptor> {
        if index >= self.count {
            return None;
        }
        self.descriptors.get(usize::from(index))
    }

    /// The oldest descriptor still owned by the device.
    #[must_use]
    pub fn peek(&self) -> Option<&Descriptor> {
        if self.used == 0 {
            return None;
        }
        self.descriptor(self.head)
    }

    /// Submit a descriptor to the device; returns its slot index.
    ///
    /// # Errors
    ///
    /// Returns [`RingError::Full`] when every slot is in flight.
    pub fn submit(&mut self, mut descriptor: Descriptor) -> Result<u16, RingError> {
        if self.is_full() {
            return Err(RingError::Full);
        }
        let index = self.tail();
        descriptor.owner = Owner::Device;
        if let Some(slot) = self.descriptors.get_mut(usize::from(index)) {
            *slot = descriptor;
        }
        self.used = self.used.saturating_add(1);
        Ok(index)
    }

    /// Reclaim the oldest completed descriptor, if the device released it.
    ///
    /// A descriptor is returned only when the slot is device-owned; the host then owns it again.
    pub fn complete(&mut self) -> Option<(u16, Descriptor)> {
        if self.used == 0 {
            return None;
        }
        let index = self.head;
        let mut descriptor = *self.descriptors.get(usize::from(index))?;
        if descriptor.owner != Owner::Device {
            return None;
        }
        if let Some(slot) = self.descriptors.get_mut(usize::from(index)) {
            slot.owner = Owner::Host;
        }
        descriptor.owner = Owner::Host;
        self.head = index.saturating_add(1) % self.count;
        self.used = self.used.saturating_sub(1);
        Some((index, descriptor))
    }
}

#[cfg(test)]
#[path = "ring_ut.rs"]
mod tests;
