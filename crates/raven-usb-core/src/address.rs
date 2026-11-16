// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USB device-address assignment.
//!
//! Address 0 is the default address used before `SET_ADDRESS`; assigned addresses are `1..=127`.
//! The pool is deterministic: it returns the lowest free address. A driver must release the
//! address when the device is detached or enumeration fails.

use core::fmt;

/// The default USB address.
pub const DEFAULT_ADDRESS: u8 = 0;
/// The highest assignable USB address.
pub const MAX_ADDRESS: u8 = 127;

/// Why an address operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressError {
    /// Every address is assigned.
    Exhausted,
    /// The address is outside `1..=127`.
    Invalid(u8),
    /// The address is already assigned.
    AlreadyAssigned(u8),
    /// The address is not assigned.
    NotAssigned(u8),
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exhausted => f.write_str("no free USB address"),
            Self::Invalid(address) => {
                write!(f, "USB address {address} is outside 1..={MAX_ADDRESS}")
            }
            Self::AlreadyAssigned(address) => {
                write!(f, "USB address {address} is already assigned")
            }
            Self::NotAssigned(address) => write!(f, "USB address {address} is not assigned"),
        }
    }
}

impl core::error::Error for AddressError {}

/// The deterministic USB address pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddressPool {
    assigned: [bool; 128],
}

impl AddressPool {
    /// An empty pool.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            assigned: [false; 128],
        }
    }

    /// Assign the lowest free address.
    ///
    /// # Errors
    ///
    /// Returns [`AddressError::Exhausted`] when all 127 addresses are in use.
    pub fn assign(&mut self) -> Result<u8, AddressError> {
        for address in 1..=MAX_ADDRESS {
            if !self.assigned[usize::from(address)] {
                self.assigned[usize::from(address)] = true;
                return Ok(address);
            }
        }
        Err(AddressError::Exhausted)
    }

    /// Release an assigned address.
    ///
    /// # Errors
    ///
    /// Returns [`AddressError::Invalid`] outside `1..=127` and
    /// [`AddressError::NotAssigned`] when the address is already free.
    pub fn release(&mut self, address: u8) -> Result<(), AddressError> {
        if address == DEFAULT_ADDRESS || address > MAX_ADDRESS {
            return Err(AddressError::Invalid(address));
        }
        if !self.assigned[usize::from(address)] {
            return Err(AddressError::NotAssigned(address));
        }
        self.assigned[usize::from(address)] = false;
        Ok(())
    }

    /// Mark a specific address as assigned.
    ///
    /// # Errors
    ///
    /// Returns [`AddressError::Invalid`] outside `1..=127` and
    /// [`AddressError::AlreadyAssigned`] when the address is taken.
    pub fn reserve(&mut self, address: u8) -> Result<(), AddressError> {
        if address == DEFAULT_ADDRESS || address > MAX_ADDRESS {
            return Err(AddressError::Invalid(address));
        }
        if self.assigned[usize::from(address)] {
            return Err(AddressError::AlreadyAssigned(address));
        }
        self.assigned[usize::from(address)] = true;
        Ok(())
    }

    /// Whether `address` is currently assigned.
    #[must_use]
    pub fn is_assigned(&self, address: u8) -> bool {
        address != DEFAULT_ADDRESS && address <= MAX_ADDRESS && self.assigned[usize::from(address)]
    }

    /// Number of assigned addresses.
    #[must_use]
    pub fn assigned(&self) -> usize {
        self.assigned.iter().filter(|assigned| **assigned).count()
    }
}

impl Default for AddressPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "address_ut.rs"]
mod tests;
