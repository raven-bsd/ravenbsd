// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Ethernet MAC address handling and the Pi 4 DTB/mailbox/hardware selection path.
//!
//! The firmware can patch `local-mac-address` into the device tree, the VideoCore mailbox can
//! return `GET_MACADDRESS`, and the UMAC registers retain the address programmed by the
//! bootloader. The resolver tries those sources in that order and only accepts a valid unicast
//! address, so a zero or broadcast value never reaches the UMAC registers.

use raven_plat::mailbox::{Mailbox, TAG_GET_MACADDRESS};

use core::fmt;

/// Length of an Ethernet MAC address.
pub const MAC_ADDRESS_LEN: usize = 6;

/// A six-byte Ethernet MAC address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacAddress(pub [u8; MAC_ADDRESS_LEN]);

impl MacAddress {
    /// Build from raw bytes.
    #[must_use]
    pub const fn new(bytes: [u8; MAC_ADDRESS_LEN]) -> Self {
        Self(bytes)
    }

    /// The raw bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; MAC_ADDRESS_LEN] {
        self.0
    }

    /// True for a unicast, non-zero, non-broadcast address.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.0[0] & 1 == 0
            && !(self.0[0] == 0
                && self.0[1] == 0
                && self.0[2] == 0
                && self.0[3] == 0
                && self.0[4] == 0
                && self.0[5] == 0)
            && !(self.0[0] == 0xff
                && self.0[1] == 0xff
                && self.0[2] == 0xff
                && self.0[3] == 0xff
                && self.0[4] == 0xff
                && self.0[5] == 0xff)
    }

    /// Decode the two little-endian words of the `GET_MACADDRESS` mailbox tag.
    #[must_use]
    pub fn from_mailbox_words(words: [u32; 2]) -> Self {
        let first = words[0].to_le_bytes();
        let second = words[1].to_le_bytes();
        Self([first[0], first[1], first[2], first[3], second[0], second[1]])
    }

    /// Encode the two UMAC registers: bytes 0..4 big-endian in `UMAC_MAC0`, bytes 4..6 in
    /// `UMAC_MAC1`.
    #[must_use]
    pub const fn umac_words(self) -> (u32, u16) {
        let mac0 = u32::from_be_bytes([self.0[0], self.0[1], self.0[2], self.0[3]]);
        let mac1 = u16::from_be_bytes([self.0[4], self.0[5]]);
        (mac0, mac1)
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d, e, f] = self.0;
        write!(formatter, "{a:02x}:{b:02x}:{c:02x}:{d:02x}:{e:02x}:{f:02x}")
    }
}

/// Where the selected MAC address came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacOrigin {
    /// A `local-mac-address`/`mac-address` property in the device tree.
    DeviceTree,
    /// The VideoCore `GET_MACADDRESS` mailbox tag.
    Mailbox,
    /// The existing UMAC registers.
    Hardware,
}

/// A selected address and its origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacSelection {
    /// The selected address.
    pub address: MacAddress,
    /// Where it came from.
    pub origin: MacOrigin,
}

/// Why no address could be selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacError {
    /// A source supplied a zero, broadcast or multicast address.
    InvalidAddress,
    /// The mailbox query failed.
    Mailbox,
    /// Every source was absent or invalid.
    NoSource,
    /// A result was supplied for a source the resolver did not ask.
    UnexpectedSource,
}

/// The next step the driver should take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacAction {
    /// Query the mailbox for `GET_MACADDRESS`.
    QueryMailbox,
    /// Read the UMAC registers.
    ReadHardware,
    /// A valid address was selected.
    Done(MacSelection),
    /// Nothing usable was found.
    Failed(MacError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueryState {
    Untried,
    Pending,
    Tried,
}

/// The deterministic DTB -> mailbox -> hardware resolver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacResolver {
    device_tree: Option<MacAddress>,
    selection: Option<MacSelection>,
    mailbox: QueryState,
    hardware: QueryState,
}

impl MacResolver {
    /// Start with the address found in the device tree, if any.
    #[must_use]
    pub const fn new(device_tree: Option<MacAddress>) -> Self {
        Self {
            device_tree,
            selection: None,
            mailbox: QueryState::Untried,
            hardware: QueryState::Untried,
        }
    }

    /// The selected address, when the resolver has finished.
    #[must_use]
    pub const fn selection(self) -> Option<MacSelection> {
        self.selection
    }

    /// Ask the resolver what to do next.
    pub fn next_action(&mut self) -> MacAction {
        if let Some(selection) = self.selection {
            return MacAction::Done(selection);
        }
        if let Some(address) = self.device_tree.take()
            && address.is_valid()
        {
            let selection = MacSelection {
                address,
                origin: MacOrigin::DeviceTree,
            };
            self.selection = Some(selection);
            return MacAction::Done(selection);
        }
        if self.mailbox == QueryState::Untried {
            self.mailbox = QueryState::Pending;
            return MacAction::QueryMailbox;
        }
        if self.hardware == QueryState::Untried {
            self.hardware = QueryState::Pending;
            return MacAction::ReadHardware;
        }
        MacAction::Failed(MacError::NoSource)
    }

    /// Supply the mailbox result after [`MacAction::QueryMailbox`].
    pub fn mailbox_result(&mut self, address: Option<MacAddress>) -> MacAction {
        if self.mailbox != QueryState::Pending {
            return MacAction::Failed(MacError::UnexpectedSource);
        }
        self.mailbox = QueryState::Tried;
        if let Some(address) = address
            && address.is_valid()
        {
            let selection = MacSelection {
                address,
                origin: MacOrigin::Mailbox,
            };
            self.selection = Some(selection);
            return MacAction::Done(selection);
        }
        self.next_action()
    }

    /// Supply the hardware result after [`MacAction::ReadHardware`].
    pub fn hardware_result(&mut self, address: Option<MacAddress>) -> MacAction {
        if self.hardware != QueryState::Pending {
            return MacAction::Failed(MacError::UnexpectedSource);
        }
        self.hardware = QueryState::Tried;
        if let Some(address) = address
            && address.is_valid()
        {
            let selection = MacSelection {
                address,
                origin: MacOrigin::Hardware,
            };
            self.selection = Some(selection);
            return MacAction::Done(selection);
        }
        self.next_action()
    }
}

/// Query `GET_MACADDRESS` through a platform mailbox.
pub fn mac_from_mailbox<M: Mailbox>(mailbox: &mut M) -> Result<MacAddress, MacError> {
    let mut values = [0u32; 2];
    let count = mailbox
        .query(TAG_GET_MACADDRESS, &[], &mut values)
        .map_err(|_| MacError::Mailbox)?;
    if count != 2 {
        return Err(MacError::Mailbox);
    }
    let address = MacAddress::from_mailbox_words(values);
    if !address.is_valid() {
        return Err(MacError::InvalidAddress);
    }
    Ok(address)
}

#[cfg(test)]
#[path = "mac_ut.rs"]
mod tests;
