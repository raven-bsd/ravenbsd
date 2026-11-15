// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PCI bus/device/function addresses.

use core::fmt;

/// Why a bus/device/function triple is not addressable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BdfError {
    /// The device number is outside `0..=31`.
    Device(u8),
    /// The function number is outside `0..=7`.
    Function(u8),
}

impl fmt::Display for BdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Device(device) => write!(f, "PCI device number {device} is outside 0..=31"),
            Self::Function(function) => {
                write!(f, "PCI function number {function} is outside 0..=7")
            }
        }
    }
}

impl core::error::Error for BdfError {}

/// One PCI bus/device/function address.
///
/// The fields are private so an invalid triple cannot reach the config-space backends; every
/// backend indexes arrays or register fields with these values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bdf {
    bus: u8,
    device: u8,
    function: u8,
}

impl Bdf {
    /// Build a validated address.
    ///
    /// # Errors
    ///
    /// Returns [`BdfError`] for a device number above 31 or a function number above 7.
    pub const fn new(bus: u8, device: u8, function: u8) -> Result<Self, BdfError> {
        if device > 31 {
            return Err(BdfError::Device(device));
        }
        if function > 7 {
            return Err(BdfError::Function(function));
        }
        Ok(Self {
            bus,
            device,
            function,
        })
    }

    /// Bus number, `0..=255`.
    #[must_use]
    pub const fn bus(self) -> u8 {
        self.bus
    }

    /// Device number, `0..=31`.
    #[must_use]
    pub const fn device(self) -> u8 {
        self.device
    }

    /// Function number, `0..=7`.
    #[must_use]
    pub const fn function(self) -> u8 {
        self.function
    }

    /// The 16-bit `bus:device:function` form (`bus << 8 | device << 3 | function`).
    #[must_use]
    pub const fn route(self) -> u16 {
        (self.bus as u16) << 8 | (self.device as u16) << 3 | self.function as u16
    }
}

impl fmt::Display for Bdf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}.{:x}",
            self.bus, self.device, self.function
        )
    }
}

#[cfg(test)]
#[path = "bdf_ut.rs"]
mod tests;
