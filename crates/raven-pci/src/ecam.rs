// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The PCI Express ECAM (enhanced configuration access mechanism) window.
//!
//! The Pi 5 firmware device tree describes the BCM2712 root complex with a 4 KiB register block
//! and no ECAM `ranges` entry, so the Pi 5 path uses the BCMstb indirect registers in
//! [`crate::indirect`]. This module still models an ECAM window because BCM2712 boards and the
//! QEMU `virt` machine can expose one, and the kernel seam must be able to use it when the tree
//! supplies it.
//!
//! Every bus occupies 1 MiB: `(bus << 20) | (device << 15) | (function << 12) | offset`.

use crate::bdf::Bdf;

/// One bus worth of ECAM address space.
pub const BUS_SPAN: u64 = 0x10_0000;
/// One function's config space.
pub const FUNCTION_SPAN: u64 = 0x1000;

/// Why an ECAM window is unusable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EcamError {
    /// The window length is zero.
    Empty,
    /// The window does not cover whole 1 MiB buses.
    UnalignedLength(u64),
    /// `first_bus` is greater than `last_bus`.
    BusOrder {
        /// The first bus in the window.
        first: u8,
        /// The last bus in the window.
        last: u8,
    },
    /// The window covers fewer bytes than its bus range needs.
    TooSmall {
        /// Buses in the declared range.
        buses: u64,
        /// Window length in bytes.
        length: u64,
    },
    /// The base or the length is not 4 KiB aligned.
    Misaligned {
        /// Offending value.
        value: u64,
    },
    /// The base plus the length overflows a 64-bit address.
    Overflow,
}

/// A validated ECAM window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ecam {
    base: u64,
    length: u64,
    first_bus: u8,
    last_bus: u8,
}

impl Ecam {
    /// Build an ECAM window.
    ///
    /// The base and length must be 4 KiB aligned (the architectural minimum page size), the
    /// length must cover every bus in `first_bus..=last_bus` at 1 MiB per bus, and the base plus
    /// the length must not overflow.
    ///
    /// # Errors
    ///
    /// Returns [`EcamError`] when any of those conditions fails.
    pub const fn new(
        base: u64,
        length: u64,
        first_bus: u8,
        last_bus: u8,
    ) -> Result<Self, EcamError> {
        if length == 0 {
            return Err(EcamError::Empty);
        }
        if first_bus > last_bus {
            return Err(EcamError::BusOrder {
                first: first_bus,
                last: last_bus,
            });
        }
        let buses = (last_bus as u64 - first_bus as u64) + 1;
        let needed = buses.saturating_mul(BUS_SPAN);
        if length < needed {
            return Err(EcamError::TooSmall { buses, length });
        }
        if !length.is_multiple_of(FUNCTION_SPAN) {
            return Err(EcamError::UnalignedLength(length));
        }
        if !base.is_multiple_of(FUNCTION_SPAN) {
            return Err(EcamError::Misaligned { value: base });
        }
        if base.checked_add(length).is_none() {
            return Err(EcamError::Overflow);
        }
        Ok(Self {
            base,
            length,
            first_bus,
            last_bus,
        })
    }

    /// CPU physical base of the window.
    #[must_use]
    pub const fn base(self) -> u64 {
        self.base
    }

    /// Window length in bytes.
    #[must_use]
    pub const fn length(self) -> u64 {
        self.length
    }

    /// First bus covered.
    #[must_use]
    pub const fn first_bus(self) -> u8 {
        self.first_bus
    }

    /// Last bus covered.
    #[must_use]
    pub const fn last_bus(self) -> u8 {
        self.last_bus
    }

    /// The window offset of a 32-bit config access, or `None` when the address or offset is
    /// outside the window.
    #[must_use]
    pub const fn offset(self, bdf: Bdf, offset: u16) -> Option<u64> {
        if bdf.bus() < self.first_bus || bdf.bus() > self.last_bus {
            return None;
        }
        if offset > 0x0FFF {
            return None;
        }
        let bus_index = (bdf.bus() - self.first_bus) as u64;
        let window = bus_index * BUS_SPAN
            + (bdf.device() as u64) * 0x8000
            + (bdf.function() as u64) * FUNCTION_SPAN
            + offset as u64;
        if window >= self.length {
            return None;
        }
        Some(window)
    }

    /// The CPU physical address of a 32-bit config access, or `None` when it is outside the
    /// window.
    ///
    /// The caller still has to reject an offset whose four bytes cross the window end; the
    /// validator sizes the window in whole buses, so a 32-bit access at `0xFFE` is inside.
    #[must_use]
    pub const fn address(self, bdf: Bdf, offset: u16) -> Option<u64> {
        match self.offset(bdf, offset) {
            Some(window) => Some(self.base + window),
            None => None,
        }
    }
}

#[cfg(test)]
#[path = "ecam_ut.rs"]
mod tests;
