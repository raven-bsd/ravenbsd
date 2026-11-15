// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The named Ethernet register map.
//!
//! The RP1 GEM register offsets are not public in this workspace (the Linux `macb` driver is the
//! usual reference and no checkout is present). The map is therefore data: the board bring-up
//! fills the offsets after reading the vendor documentation or a board trace, and the rest of the
//! model works with names. Offsets are 32-bit aligned and relative to the Ethernet window.

use core::fmt;

/// Number of named registers.
pub const REGISTER_COUNT: usize = 16;
/// Ethernet register window from the device tree (`0x4000` bytes).
pub const WINDOW_BYTES: u16 = 0x4000;

/// One named register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Register {
    /// MAC control.
    Control,
    /// MAC status.
    Status,
    /// Low MAC address word.
    MacAddressLow,
    /// High MAC address word.
    MacAddressHigh,
    /// Filter mode.
    Filter,
    /// Low multicast hash word.
    HashLow,
    /// High multicast hash word.
    HashHigh,
    /// MDIO command.
    MdioControl,
    /// MDIO data.
    MdioData,
    /// TX descriptor ring base.
    TxRing,
    /// RX descriptor ring base.
    RxRing,
    /// Interrupt status.
    InterruptStatus,
    /// Interrupt mask.
    InterruptMask,
    /// Interrupt acknowledge.
    InterruptAck,
    /// PHY configuration.
    PhyConfig,
    /// Global configuration.
    Config,
}

impl Register {
    /// All registers in index order.
    pub const ALL: [Self; REGISTER_COUNT] = [
        Self::Control,
        Self::Status,
        Self::MacAddressLow,
        Self::MacAddressHigh,
        Self::Filter,
        Self::HashLow,
        Self::HashHigh,
        Self::MdioControl,
        Self::MdioData,
        Self::TxRing,
        Self::RxRing,
        Self::InterruptStatus,
        Self::InterruptMask,
        Self::InterruptAck,
        Self::PhyConfig,
        Self::Config,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Control => 0,
            Self::Status => 1,
            Self::MacAddressLow => 2,
            Self::MacAddressHigh => 3,
            Self::Filter => 4,
            Self::HashLow => 5,
            Self::HashHigh => 6,
            Self::MdioControl => 7,
            Self::MdioData => 8,
            Self::TxRing => 9,
            Self::RxRing => 10,
            Self::InterruptStatus => 11,
            Self::InterruptMask => 12,
            Self::InterruptAck => 13,
            Self::PhyConfig => 14,
            Self::Config => 15,
        }
    }
}

/// Why a register offset is rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterError {
    /// The offset is not 32-bit aligned.
    Misaligned(u16),
    /// The offset leaves the 4 KiB Ethernet register window.
    OutOfWindow(u16),
}

impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Misaligned(offset) => {
                write!(f, "register offset {offset:#x} is not 32-bit aligned")
            }
            Self::OutOfWindow(offset) => {
                write!(
                    f,
                    "register offset {offset:#x} is outside the {WINDOW_BYTES:#x} window"
                )
            }
        }
    }
}

impl core::error::Error for RegisterError {}

/// The offsets of the named registers, where known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegisterMap {
    offsets: [Option<u16>; REGISTER_COUNT],
}

impl RegisterMap {
    /// An empty map.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            offsets: [None; REGISTER_COUNT],
        }
    }

    /// Set a register offset.
    ///
    /// # Errors
    ///
    /// Returns [`RegisterError::Misaligned`] unless the offset is 32-bit aligned and
    /// [`RegisterError::OutOfWindow`] for an offset whose word leaves the register window.
    pub fn set(&mut self, register: Register, offset: u16) -> Result<(), RegisterError> {
        if offset & 3 != 0 {
            return Err(RegisterError::Misaligned(offset));
        }
        if offset > WINDOW_BYTES - 4 {
            return Err(RegisterError::OutOfWindow(offset));
        }
        if let Some(slot) = self.offsets.get_mut(register.index()) {
            *slot = Some(offset);
        }
        Ok(())
    }

    /// Clear a register offset.
    pub fn clear(&mut self, register: Register) {
        if let Some(slot) = self.offsets.get_mut(register.index()) {
            *slot = None;
        }
    }

    /// The offset of `register`, when set.
    #[must_use]
    pub fn offset(&self, register: Register) -> Option<u16> {
        self.offsets.get(register.index()).copied().flatten()
    }

    /// Number of registers with an offset.
    #[must_use]
    pub fn len(&self) -> usize {
        self.offsets
            .iter()
            .filter(|offset| offset.is_some())
            .count()
    }

    /// True when no register has an offset.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.offsets.iter().all(Option::is_none)
    }
}

impl Default for RegisterMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "regs_ut.rs"]
mod tests;
