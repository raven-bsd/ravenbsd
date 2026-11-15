// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Typed PCI config-space access.
//!
//! [`ConfigSpace`] is the only hardware boundary in the crate: the kernel seam implements it over
//! the BCMstb indirect registers, ECAM or a test double. Everything above it is pure data.

use core::fmt;

use crate::bdf::Bdf;

/// Vendor/device ID register.
pub const REG_VENDOR_DEVICE: u16 = 0x00;
/// Command/status register pair.
pub const REG_COMMAND_STATUS: u16 = 0x04;
/// Revision (low byte) and class code register.
pub const REG_REVISION_CLASS: u16 = 0x08;
/// Cache line, latency, header and BIST bytes.
pub const REG_HEADER: u16 = 0x0C;
/// First base-address register.
pub const REG_BAR0: u16 = 0x10;
/// Capability-list pointer (status bit 4 says whether it is valid).
pub const REG_CAPABILITY: u16 = 0x34;
/// Interrupt line and pin bytes.
pub const REG_INTERRUPT: u16 = 0x3C;
/// One function's config space.
pub const CONFIG_SPACE_SIZE: u16 = 0x1000;
/// The all-ones word a config space returns for a missing function.
pub const NO_DEVICE: u32 = 0xFFFF_FFFF;

/// Command-register bits used by the enumeration path.
pub const COMMAND_IO_SPACE: u16 = 1 << 0;
/// Enables the memory BARs.
pub const COMMAND_MEMORY_SPACE: u16 = 1 << 1;
/// Enables DMA.
pub const COMMAND_BUS_MASTER: u16 = 1 << 2;
/// Disables INTx assertion.
pub const COMMAND_INTERRUPT_DISABLE: u16 = 1 << 10;

/// Status-register bit announcing a capability list.
pub const STATUS_CAPABILITIES: u16 = 1 << 4;
/// PCI Express capability ID.
pub const CAPABILITY_PCIE: u8 = 0x10;

/// Capability-list walk bound; a malformed list must not loop forever.
pub const MAX_CAPABILITIES: u32 = 48;

/// A 32-bit config-space backend.
///
/// Real hardware has two access paths: ECAM (a memory window) and the BCMstb indirect
/// index/data pair. Both expose 32-bit words; the byte and halfword helpers below derive from
/// them exactly as the PCI specification requires (lower address bits select the lane).
pub trait ConfigSpace {
    /// Read the 32-bit word at `offset` (low two bits select the lane).
    fn read32(&mut self, bdf: Bdf, offset: u16) -> u32;

    /// Write the 32-bit word at `offset`.
    fn write32(&mut self, bdf: Bdf, offset: u16, value: u32);

    /// Read a 16-bit halfword.
    fn read16(&mut self, bdf: Bdf, offset: u16) -> u16 {
        let word = self.read32(bdf, offset & !3);
        let shift = u32::from(offset & 2) * 8;
        (word >> shift) as u16
    }

    /// Write a 16-bit halfword with a read-modify-write of its word.
    fn write16(&mut self, bdf: Bdf, offset: u16, value: u16) {
        let aligned = offset & !3;
        let shift = u32::from(offset & 2) * 8;
        let mask = 0xFFFF_u32 << shift;
        let word = self.read32(bdf, aligned);
        let merged = (word & !mask) | (u32::from(value) << shift);
        self.write32(bdf, aligned, merged);
    }

    /// Read an 8-bit byte.
    fn read8(&mut self, bdf: Bdf, offset: u16) -> u8 {
        let word = self.read32(bdf, offset & !3);
        let shift = u32::from(offset & 3) * 8;
        (word >> shift) as u8
    }

    /// Write an 8-bit byte with a read-modify-write of its word.
    fn write8(&mut self, bdf: Bdf, offset: u16, value: u8) {
        let aligned = offset & !3;
        let shift = u32::from(offset & 3) * 8;
        let mask = 0xFF_u32 << shift;
        let word = self.read32(bdf, aligned);
        let merged = (word & !mask) | (u32::from(value) << shift);
        self.write32(bdf, aligned, merged);
    }
}

/// Vendor and device identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    /// Vendor ID.
    pub vendor: u16,
    /// Device ID.
    pub device: u16,
}

/// Class code and revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Class {
    /// Revision ID.
    pub revision: u8,
    /// Base class (the high byte of the 24-bit class code).
    pub base: u8,
    /// Subclass.
    pub sub: u8,
    /// Programming interface.
    pub interface: u8,
}

/// The header type of one function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderKind {
    /// Type 0: a normal function.
    Endpoint,
    /// Type 1: a PCI-to-PCI bridge.
    Bridge,
    /// Type 2: a CardBus bridge (legacy, not expected on PCIe).
    Cardbus,
    /// A value the specification does not define; kept for diagnostics.
    Unknown(u8),
}

impl fmt::Display for HeaderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Endpoint => f.write_str("endpoint"),
            Self::Bridge => f.write_str("bridge"),
            Self::Cardbus => f.write_str("cardbus"),
            Self::Unknown(value) => write!(f, "unknown({value})"),
        }
    }
}

/// The header-type byte split into its two fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Header layout.
    pub kind: HeaderKind,
    /// Bit 7 of the header byte: more than one function is present.
    pub multifunction: bool,
}

/// Read the vendor/device pair, or `None` when no function responds.
///
/// A single-function device answers for every function number with the same ID while
/// `multifunction` is clear; callers that only want the first function keep using function 0.
#[must_use]
pub fn identity<C: ConfigSpace + ?Sized>(config: &mut C, bdf: Bdf) -> Option<Identity> {
    let word = config.read32(bdf, REG_VENDOR_DEVICE);
    if word == NO_DEVICE {
        return None;
    }
    Some(Identity {
        vendor: word as u16,
        device: (word >> 16) as u16,
    })
}

/// Read the revision and class code.
///
/// The class word is returned verbatim so a caller can compare it against a known value, and the
/// split fields are exposed by [`Class`].
#[must_use]
pub fn class<C: ConfigSpace + ?Sized>(config: &mut C, bdf: Bdf) -> (u32, Class) {
    let word = config.read32(bdf, REG_REVISION_CLASS);
    (
        word >> 8,
        Class {
            revision: word as u8,
            base: (word >> 24) as u8,
            sub: (word >> 16) as u8,
            interface: (word >> 8) as u8,
        },
    )
}

/// Read the header-type byte.
#[must_use]
pub fn header<C: ConfigSpace + ?Sized>(config: &mut C, bdf: Bdf) -> Header {
    let value = config.read8(bdf, REG_HEADER + 2);
    let kind = match value & 0x7F {
        0 => HeaderKind::Endpoint,
        1 => HeaderKind::Bridge,
        2 => HeaderKind::Cardbus,
        other => HeaderKind::Unknown(other),
    };
    Header {
        kind,
        multifunction: value & 0x80 != 0,
    }
}

/// Read the command register.
#[must_use]
pub fn command<C: ConfigSpace + ?Sized>(config: &mut C, bdf: Bdf) -> u16 {
    config.read16(bdf, REG_COMMAND_STATUS)
}

/// Enable memory decoding and bus mastering.
///
/// The RP1 endpoint must decode its BAR window before the host touches any peripheral register,
/// and DMA comes later; both bits stay set once configured.
#[must_use]
pub fn enable_memory_and_bus_master<C: ConfigSpace + ?Sized>(config: &mut C, bdf: Bdf) -> u16 {
    let value = command(config, bdf) | COMMAND_MEMORY_SPACE | COMMAND_BUS_MASTER;
    config.write16(bdf, REG_COMMAND_STATUS, value);
    value
}

/// Find a capability by ID and return its byte offset.
///
/// The walk is bounded by [`MAX_CAPABILITIES`] and rejects a pointer below `0x40`, a zero next
/// pointer, a self-loop and a truncated final entry. The status register's capability bit must be
/// set, otherwise the pointer is reserved.
#[must_use]
pub fn find_capability<C: ConfigSpace + ?Sized>(config: &mut C, bdf: Bdf, id: u8) -> Option<u8> {
    let status = config.read16(bdf, REG_COMMAND_STATUS + 2);
    if status & STATUS_CAPABILITIES == 0 {
        return None;
    }
    let mut pointer = config.read8(bdf, REG_CAPABILITY) & 0xFC;
    let mut steps = 0u32;
    while pointer >= 0x40 && steps < MAX_CAPABILITIES {
        let capability = config.read8(bdf, u16::from(pointer));
        if capability == id {
            return Some(pointer);
        }
        let next = config.read8(bdf, u16::from(pointer) + 1) & 0xFC;
        if next == 0 || next == pointer {
            return None;
        }
        pointer = next;
        steps = steps.saturating_add(1);
    }
    None
}

#[cfg(test)]
#[path = "config_ut.rs"]
mod tests;
