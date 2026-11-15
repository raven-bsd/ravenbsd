// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! UMAC unicast, broadcast and multicast filter programming.
//!
//! GENET has 17 multicast-destination-filter slots. Slot 0 carries the broadcast address, slot 1
//! the station address, and slots 2..16 carry up to 15 multicast addresses. `UMAC_MDF_CTRL`
//! enables a contiguous mask from the top; `UMAC_CMD_PROMISC` bypasses the table.

use crate::mac::MacAddress;
use crate::regs::{MDF_FILTERS, UMAC_CMD_PROMISC};

/// Number of hardware filter slots.
pub const MDF_SLOTS: usize = MDF_FILTERS as usize;
/// Largest multicast list the table can hold after broadcast and the station address.
pub const MAX_MULTICAST: usize = MDF_SLOTS - 2;
/// The Ethernet broadcast address.
pub const BROADCAST: MacAddress = MacAddress::new([0xff; 6]);

/// One filter slot write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FilterWrite {
    /// Slot number, 0..16.
    pub slot: u8,
    /// First MDF address word.
    pub addr0: u32,
    /// Second MDF address word.
    pub addr1: u32,
}

/// Why a filter table could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterError {
    /// More multicast addresses than the hardware slots can hold.
    TooManyMulticast {
        /// Addresses supplied.
        provided: usize,
        /// Addresses the table can hold.
        capacity: usize,
    },
}

/// Encode one MAC address into the two MDF words.
#[must_use]
pub const fn mdf_words(address: MacAddress) -> (u32, u32) {
    let bytes = address.bytes();
    let addr0 = ((bytes[0] as u32) << 8) | bytes[1] as u32;
    let addr1 = ((bytes[2] as u32) << 24)
        | ((bytes[3] as u32) << 16)
        | ((bytes[4] as u32) << 8)
        | bytes[5] as u32;
    (addr0, addr1)
}

/// The broadcast, station and multicast filter table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FilterTable {
    own: MacAddress,
    multicast: [Option<MacAddress>; MAX_MULTICAST],
    count: usize,
    promiscuous: bool,
}

impl FilterTable {
    /// Create a table with the station address and no multicast entries.
    #[must_use]
    pub const fn new(own: MacAddress) -> Self {
        Self {
            own,
            multicast: [None; MAX_MULTICAST],
            count: 0,
            promiscuous: false,
        }
    }

    /// The station address.
    #[must_use]
    pub const fn own(self) -> MacAddress {
        self.own
    }

    /// Replace the multicast list.
    pub fn set_multicast(&mut self, addresses: &[MacAddress]) -> Result<(), FilterError> {
        if addresses.len() > MAX_MULTICAST {
            return Err(FilterError::TooManyMulticast {
                provided: addresses.len(),
                capacity: MAX_MULTICAST,
            });
        }
        self.multicast = [None; MAX_MULTICAST];
        for (slot, address) in self.multicast.iter_mut().zip(addresses.iter()) {
            *slot = Some(*address);
        }
        self.count = addresses.len();
        Ok(())
    }

    /// Drop all multicast entries.
    pub fn clear_multicast(&mut self) {
        self.multicast = [None; MAX_MULTICAST];
        self.count = 0;
    }

    /// Set or clear promiscuous mode.
    pub const fn set_promiscuous(&mut self, enabled: bool) {
        self.promiscuous = enabled;
    }

    /// True when promiscuous mode is enabled.
    #[must_use]
    pub const fn is_promiscuous(self) -> bool {
        self.promiscuous
    }

    /// Number of used slots: broadcast + station + multicast.
    #[must_use]
    pub const fn slot_count(self) -> u8 {
        (2 + self.count) as u8
    }

    /// The value for `UMAC_MDF_CTRL`.
    #[must_use]
    pub const fn mdf_ctrl(self) -> u32 {
        let used = self.slot_count() as u32;
        if used == 0 {
            0
        } else {
            ((1u32 << used) - 1) << (MDF_SLOTS as u32 - used)
        }
    }

    /// `UMAC_CMD` bits this table needs.
    #[must_use]
    pub const fn command_bits(self) -> u32 {
        if self.promiscuous {
            UMAC_CMD_PROMISC
        } else {
            0
        }
    }

    /// The write for `slot`, or `None` when the slot is unused.
    #[must_use]
    pub fn write(self, slot: u8) -> Option<FilterWrite> {
        let address = match slot {
            0 => Some(BROADCAST),
            1 => Some(self.own),
            _ => {
                let index = usize::from(slot).checked_sub(2)?;
                if index >= self.count {
                    return None;
                }
                self.multicast.get(index).copied().flatten()
            }
        }?;
        let (addr0, addr1) = mdf_words(address);
        Some(FilterWrite { slot, addr0, addr1 })
    }
}

#[cfg(test)]
#[path = "filter_ut.rs"]
mod tests;
