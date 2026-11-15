// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The BCMstb indirect config-space protocol.
//!
//! The BCM2711 (Pi 4) and BCM2712 (Pi 5) root complexes do not decode an ECAM window in the
//! firmware device tree. Config cycles go through two controller registers instead: write the
//! bus/device/function word to the index register, then read or write the data register at
//! `EXT_CFG_DATA + offset`. Bus 0 (the root complex's own type-1 header) is accessed directly at
//! `offset` in the register window.
//!
//! The offsets are the ones the NetBSD `bcm2838_pcie` driver uses for BCM2711. The BCM2712
//! values are **UNVERIFIED** until a Pi 5 board log confirms them; they are data here so a board
//! bring-up can correct them without changing the protocol.

use crate::bdf::Bdf;
use crate::config::{CONFIG_SPACE_SIZE, ConfigSpace, NO_DEVICE};

/// Register offset of the indirect config data word (BCM2711 observation).
pub const EXT_CFG_DATA: u32 = 0x8000;
/// Register offset of the indirect config index word (BCM2711 observation).
pub const EXT_CFG_INDEX: u32 = 0x9000;

/// Bit position of the bus number in the index word.
pub const INDEX_BUS_SHIFT: u32 = 20;
/// Bit position of the device number in the index word.
pub const INDEX_DEVICE_SHIFT: u32 = 15;
/// Bit position of the function number in the index word.
pub const INDEX_FUNCTION_SHIFT: u32 = 12;
/// Mask of the bus field.
pub const INDEX_BUS_MASK: u32 = 0xFF;
/// Mask of the device field.
pub const INDEX_DEVICE_MASK: u32 = 0x1F;
/// Mask of the function field.
pub const INDEX_FUNCTION_MASK: u32 = 0x7;

/// A window of 32-bit controller registers.
///
/// The kernel seam implements this over mapped device memory; the simulator implements it in
/// RAM. The trait exists so the indirect protocol itself stays host-testable.
pub trait RegisterWindow {
    /// Read the 32-bit register at `offset`.
    fn read32(&mut self, offset: u32) -> u32;

    /// Write the 32-bit register at `offset`.
    fn write32(&mut self, offset: u32, value: u32);
}

/// Encode the index word for `bdf`.
#[must_use]
pub const fn index_word(bdf: Bdf) -> u32 {
    ((bdf.bus() as u32 & INDEX_BUS_MASK) << INDEX_BUS_SHIFT)
        | ((bdf.device() as u32 & INDEX_DEVICE_MASK) << INDEX_DEVICE_SHIFT)
        | ((bdf.function() as u32 & INDEX_FUNCTION_MASK) << INDEX_FUNCTION_SHIFT)
}

/// The data register offset for a config access on `bus`.
///
/// Bus 0 addresses the root complex's own header directly in the register window; every other
/// bus goes through the indirect page.
#[must_use]
pub const fn data_register(bus: u8, offset: u16) -> u32 {
    if bus == 0 {
        (offset as u32) & 0xFFC
    } else {
        EXT_CFG_DATA + ((offset as u32) & 0xFFC)
    }
}

/// A [`ConfigSpace`] over the two indirect registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Indirect<W: RegisterWindow> {
    window: W,
}

impl<W: RegisterWindow> Indirect<W> {
    /// Take ownership of the controller register window.
    pub const fn new(window: W) -> Self {
        Self { window }
    }

    /// Return the register window.
    pub fn release(self) -> W {
        self.window
    }
}

impl<W: RegisterWindow> ConfigSpace for Indirect<W> {
    fn read32(&mut self, bdf: Bdf, offset: u16) -> u32 {
        if offset >= CONFIG_SPACE_SIZE {
            return NO_DEVICE;
        }
        self.window.write32(EXT_CFG_INDEX, index_word(bdf));
        self.window.read32(data_register(bdf.bus(), offset))
    }

    fn write32(&mut self, bdf: Bdf, offset: u16, value: u32) {
        if offset >= CONFIG_SPACE_SIZE {
            return;
        }
        self.window.write32(EXT_CFG_INDEX, index_word(bdf));
        self.window.write32(data_register(bdf.bus(), offset), value);
    }
}

#[cfg(test)]
#[path = "indirect_ut.rs"]
mod tests;
