// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Ethernet II framing.

use core::fmt;

use crate::ParseError;

/// Length of an Ethernet II header, excluding the frame check sequence.
pub const HEADER_LEN: usize = 14;

/// A 48-bit Ethernet hardware address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MacAddr([u8; 6]);

impl MacAddr {
    /// The broadcast address `ff:ff:ff:ff:ff:ff`.
    pub const BROADCAST: Self = Self([0xff; 6]);

    /// Construct an address from its six octets.
    #[must_use]
    pub const fn new(octets: [u8; 6]) -> Self {
        Self(octets)
    }

    /// Return the six octets in wire order.
    #[must_use]
    pub const fn octets(self) -> [u8; 6] {
        self.0
    }

    /// Whether this is the broadcast address.
    #[must_use]
    pub const fn is_broadcast(self) -> bool {
        matches!(self.0, [0xff, 0xff, 0xff, 0xff, 0xff, 0xff])
    }

    /// Whether this is a group (multicast/broadcast) address.
    #[must_use]
    pub const fn is_group(self) -> bool {
        self.0[0] & 0x01 != 0
    }
}

impl From<[u8; 6]> for MacAddr {
    fn from(octets: [u8; 6]) -> Self {
        Self(octets)
    }
}

impl fmt::Display for MacAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d, e, g] = self.0;
        write!(f, "{a:02x}:{b:02x}:{c:02x}:{d:02x}:{e:02x}:{g:02x}")
    }
}

/// An Ethernet II EtherType.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EtherType {
    /// IPv4 (`0x0800`).
    Ipv4,
    /// ARP (`0x0806`).
    Arp,
    /// IPv6 (`0x86dd`).
    Ipv6,
    /// Any other EtherType.
    Unknown(u16),
}

impl EtherType {
    /// Construct an EtherType from its wire value.
    #[must_use]
    pub const fn from_u16(value: u16) -> Self {
        match value {
            0x0800 => Self::Ipv4,
            0x0806 => Self::Arp,
            0x86dd => Self::Ipv6,
            other => Self::Unknown(other),
        }
    }

    /// Return the wire value.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        match self {
            Self::Ipv4 => 0x0800,
            Self::Arp => 0x0806,
            Self::Ipv6 => 0x86dd,
            Self::Unknown(value) => value,
        }
    }
}

impl fmt::Display for EtherType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ipv4 => f.write_str("IPv4"),
            Self::Arp => f.write_str("ARP"),
            Self::Ipv6 => f.write_str("IPv6"),
            Self::Unknown(value) => write!(f, "0x{value:04x}"),
        }
    }
}

/// A borrowed Ethernet II frame.
#[derive(Debug, Clone, Copy)]
pub struct EthernetFrame<'a> {
    bytes: &'a [u8],
}

impl<'a> EthernetFrame<'a> {
    /// Parse the frame header.
    ///
    /// # Errors
    /// Returns `ethernet.truncated` when fewer than [`HEADER_LEN`] octets are supplied.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < HEADER_LEN {
            return Err(ParseError::Truncated {
                tag: "ethernet.truncated",
                need: HEADER_LEN,
                got: bytes.len(),
            });
        }
        Ok(Self { bytes })
    }

    /// Destination hardware address.
    #[must_use]
    pub fn destination(&self) -> MacAddr {
        let mut octets = [0u8; 6];
        if let Some(part) = self.bytes.get(0..6) {
            octets.copy_from_slice(part);
        }
        MacAddr::new(octets)
    }

    /// Source hardware address.
    #[must_use]
    pub fn source(&self) -> MacAddr {
        let mut octets = [0u8; 6];
        if let Some(part) = self.bytes.get(6..12) {
            octets.copy_from_slice(part);
        }
        MacAddr::new(octets)
    }

    /// The payload EtherType.
    #[must_use]
    pub fn ether_type(&self) -> EtherType {
        match self.bytes.get(12..14) {
            Some(&[hi, lo]) => EtherType::from_u16(u16::from_be_bytes([hi, lo])),
            _ => EtherType::Unknown(0),
        }
    }

    /// The bytes after the Ethernet header.
    #[must_use]
    pub fn payload(&self) -> &'a [u8] {
        self.bytes.get(HEADER_LEN..).unwrap_or(&[])
    }

    /// The complete frame.
    #[must_use]
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

/// Builder for an Ethernet II frame written into a caller-owned buffer.
#[derive(Debug)]
pub struct EthernetBuilder<'a> {
    buffer: &'a mut [u8],
    total_len: usize,
}

impl<'a> EthernetBuilder<'a> {
    /// Write the Ethernet header for a payload of `payload_len` octets.
    ///
    /// The payload area is left for the caller to fill via [`Self::payload_mut`].
    ///
    /// # Errors
    /// Returns `ethernet.truncated` when the buffer cannot hold the header and payload.
    pub fn new(
        buffer: &'a mut [u8],
        destination: MacAddr,
        source: MacAddr,
        ether_type: EtherType,
        payload_len: usize,
    ) -> Result<Self, ParseError> {
        let total_len = HEADER_LEN
            .checked_add(payload_len)
            .ok_or(ParseError::Truncated {
                tag: "ethernet.truncated",
                need: usize::MAX,
                got: buffer.len(),
            })?;
        if buffer.len() < total_len {
            return Err(ParseError::Truncated {
                tag: "ethernet.truncated",
                need: total_len,
                got: buffer.len(),
            });
        }
        let Some(header) = buffer.get_mut(..HEADER_LEN) else {
            return Err(ParseError::Truncated {
                tag: "ethernet.truncated",
                need: total_len,
                got: buffer.len(),
            });
        };
        header[..6].copy_from_slice(&destination.octets());
        header[6..12].copy_from_slice(&source.octets());
        header[12..14].copy_from_slice(&ether_type.as_u16().to_be_bytes());
        Ok(Self { buffer, total_len })
    }

    /// The payload area.
    pub fn payload_mut(&mut self) -> &mut [u8] {
        match self.buffer.get_mut(HEADER_LEN..self.total_len) {
            Some(payload) => payload,
            None => &mut [],
        }
    }

    /// Finish the frame and return the written bytes.
    pub fn finish(self) -> &'a mut [u8] {
        match self.buffer.get_mut(..self.total_len) {
            Some(frame) => frame,
            None => &mut [],
        }
    }
}
