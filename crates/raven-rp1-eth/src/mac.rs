// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Ethernet MAC addresses and the receive filter.
//!
//! The exact-match entries and the destination classification are standard Ethernet semantics. The
//! hash filter is modeled with the standard IEEE CRC-32 over the six address bytes; which six hash
//! bits the RP1 GEM exposes is UNVERIFIED, so the low six bits are the model's choice and the
//! board bring-up may change the mapping without touching the filter logic.

use core::fmt;

/// Length of a MAC address in bytes.
pub const MAC_LEN: usize = 6;
/// Largest number of exact-match entries.
pub const MAX_EXACT: usize = 8;

/// Why a MAC address or filter operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacError {
    /// The text is not six colon-separated hexadecimal octets.
    Format,
    /// The byte slice is not six bytes long.
    Length(usize),
    /// An exact entry is multicast or broadcast.
    NotUnicast,
}

impl fmt::Display for MacError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format => f.write_str("MAC address is not six colon-separated hex octets"),
            Self::Length(length) => write!(f, "MAC address has {length} bytes, not {MAC_LEN}"),
            Self::NotUnicast => f.write_str("an exact filter entry must be a unicast address"),
        }
    }
}

impl core::error::Error for MacError {}

/// A 48-bit Ethernet address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MacAddress {
    octets: [u8; MAC_LEN],
}

impl MacAddress {
    /// Build an address from its six octets.
    #[must_use]
    pub const fn new(octets: [u8; MAC_LEN]) -> Self {
        Self { octets }
    }

    /// Build an address from a slice.
    ///
    /// # Errors
    ///
    /// Returns [`MacError::Length`] when the slice is not six bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MacError> {
        let octets: [u8; MAC_LEN] = bytes
            .try_into()
            .map_err(|_| MacError::Length(bytes.len()))?;
        Ok(Self { octets })
    }

    /// Parse `aa:bb:cc:dd:ee:ff` (hexadecimal, case-insensitive).
    ///
    /// # Errors
    ///
    /// Returns [`MacError::Format`] for any other shape.
    pub fn parse(text: &str) -> Result<Self, MacError> {
        let mut octets = [0u8; MAC_LEN];
        let mut parts = text.split(':');
        for octet in &mut octets {
            let part = parts.next().ok_or(MacError::Format)?;
            if part.len() != 2 || !part.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(MacError::Format);
            }
            *octet = u8::from_str_radix(part, 16).map_err(|_| MacError::Format)?;
        }
        if parts.next().is_some() {
            return Err(MacError::Format);
        }
        Ok(Self { octets })
    }

    /// The six octets, most significant first.
    #[must_use]
    pub const fn octets(self) -> [u8; MAC_LEN] {
        self.octets
    }

    /// True when the group bit is clear.
    #[must_use]
    pub const fn is_unicast(self) -> bool {
        self.octets[0] & 0x01 == 0
    }

    /// True when the group bit is set.
    #[must_use]
    pub const fn is_multicast(self) -> bool {
        self.octets[0] & 0x01 != 0
    }

    /// True for `ff:ff:ff:ff:ff:ff`.
    #[must_use]
    pub const fn is_broadcast(self) -> bool {
        self.octets[0] == 0xFF
            && self.octets[1] == 0xFF
            && self.octets[2] == 0xFF
            && self.octets[3] == 0xFF
            && self.octets[4] == 0xFF
            && self.octets[5] == 0xFF
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.octets[0],
            self.octets[1],
            self.octets[2],
            self.octets[3],
            self.octets[4],
            self.octets[5]
        )
    }
}

/// The standard IEEE CRC-32 (reflected, polynomial `0xedb8_8320`).
#[must_use]
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Which frames the filter accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    /// Broadcast, own unicast entries and matching multicast hash bits.
    Normal,
    /// Every frame.
    Promiscuous,
    /// Every multicast frame plus broadcast and own unicast entries.
    AllMulticast,
}

/// The receive filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacFilter {
    mode: FilterMode,
    exact: [Option<MacAddress>; MAX_EXACT],
    count: usize,
    hash: u64,
}

impl MacFilter {
    /// An empty filter in [`FilterMode::Normal`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            mode: FilterMode::Normal,
            exact: [None; MAX_EXACT],
            count: 0,
            hash: 0,
        }
    }

    /// Set the mode.
    pub fn set_mode(&mut self, mode: FilterMode) {
        self.mode = mode;
    }

    /// The current mode.
    #[must_use]
    pub const fn mode(&self) -> FilterMode {
        self.mode
    }

    /// Add an exact-match unicast entry.
    ///
    /// Returns `false` for a duplicate, a multicast/broadcast address or a full table.
    pub fn add_exact(&mut self, address: MacAddress) -> bool {
        if !address.is_unicast() || self.exact.contains(&Some(address)) {
            return false;
        }
        match self.exact.get_mut(self.count) {
            Some(slot) => {
                *slot = Some(address);
                self.count = self.count.saturating_add(1);
                true
            }
            None => false,
        }
    }

    /// Remove an exact-match entry.
    pub fn remove_exact(&mut self, address: MacAddress) -> bool {
        let Some(index) = self
            .exact
            .iter()
            .take(self.count)
            .position(|entry| *entry == Some(address))
        else {
            return false;
        };
        if index < self.count {
            let mut position = index;
            while position.saturating_add(1) < self.count {
                let next = self
                    .exact
                    .get(position.saturating_add(1))
                    .copied()
                    .flatten();
                if let Some(slot) = self.exact.get_mut(position) {
                    *slot = next;
                }
                position = position.saturating_add(1);
            }
            if let Some(last) = self.exact.get_mut(self.count.saturating_sub(1)) {
                *last = None;
            }
            self.count = self.count.saturating_sub(1);
        }
        true
    }

    /// Number of exact entries.
    #[must_use]
    pub const fn exact_len(&self) -> usize {
        self.count
    }

    /// Replace the multicast hash table.
    pub fn set_hash(&mut self, hash: u64) {
        self.hash = hash;
    }

    /// Set the hash bit for `address` in the model's table.
    pub fn add_hash(&mut self, address: MacAddress) {
        self.hash |= 1u64 << usize::from(Self::hash_bit(address));
    }

    /// The multicast hash table.
    #[must_use]
    pub const fn hash(&self) -> u64 {
        self.hash
    }

    /// The model's hash bit for a multicast address: the low six bits of the IEEE CRC-32.
    ///
    /// The real RP1 hash bit selection is UNVERIFIED; only the low-six-bits mapping is tested.
    #[must_use]
    pub fn hash_bit(address: MacAddress) -> u8 {
        (crc32(&address.octets()) & 0x3F) as u8
    }

    /// Decide whether a frame addressed to `destination` passes the filter.
    #[must_use]
    pub fn accepts(&self, destination: MacAddress) -> bool {
        if destination.is_broadcast() {
            return true;
        }
        if self.mode == FilterMode::Promiscuous {
            return true;
        }
        if destination.is_multicast() {
            if self.mode == FilterMode::AllMulticast {
                return true;
            }
            let bit = Self::hash_bit(destination);
            return self.hash & (1u64 << usize::from(bit)) != 0;
        }
        self.exact
            .iter()
            .take(self.count)
            .flatten()
            .any(|entry| *entry == destination)
    }
}

impl Default for MacFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "mac_ut.rs"]
mod tests;
