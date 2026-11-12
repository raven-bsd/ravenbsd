// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Ethernet/IPv4 ARP parsing and construction (RFC 826).

use crate::ethernet::MacAddr;
use crate::ipv4::Ipv4Addr;
use crate::{ParseError, read_u8, read_u16, slice};

/// Length of an Ethernet/IPv4 ARP packet.
pub const PACKET_LEN: usize = 28;

/// A borrowed Ethernet/IPv4 ARP packet.
#[derive(Debug, Clone, Copy)]
pub struct ArpPacket<'a> {
    bytes: &'a [u8],
}

impl<'a> ArpPacket<'a> {
    /// Parse and validate an ARP packet for Ethernet and IPv4.
    ///
    /// Unknown operation codes parse successfully; the interface ignores them.
    ///
    /// # Errors
    /// Returns `arp.truncated`, `arp.htype`, `arp.ptype`, `arp.hlen` or `arp.plen`.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        let packet = slice(bytes, 0, PACKET_LEN, "arp.truncated")?;
        if read_u16(packet, 0, "arp.truncated")? != 1 {
            return Err(ParseError::Malformed { tag: "arp.htype" });
        }
        if read_u16(packet, 2, "arp.truncated")? != 0x0800 {
            return Err(ParseError::Malformed { tag: "arp.ptype" });
        }
        if read_u8(packet, 4, "arp.truncated")? != 6 {
            return Err(ParseError::Malformed { tag: "arp.hlen" });
        }
        if read_u8(packet, 5, "arp.truncated")? != 4 {
            return Err(ParseError::Malformed { tag: "arp.plen" });
        }
        Ok(Self { bytes: packet })
    }

    /// Hardware type (always 1 after a successful parse).
    #[must_use]
    pub fn hardware_type(&self) -> u16 {
        read_u16(self.bytes, 0, "arp.truncated").unwrap_or(0)
    }

    /// Protocol type (always `0x0800` after a successful parse).
    #[must_use]
    pub fn protocol_type(&self) -> u16 {
        read_u16(self.bytes, 2, "arp.truncated").unwrap_or(0)
    }

    /// Hardware address length (always 6 after a successful parse).
    #[must_use]
    pub fn hardware_len(&self) -> u8 {
        read_u8(self.bytes, 4, "arp.truncated").unwrap_or(0)
    }

    /// Protocol address length (always 4 after a successful parse).
    #[must_use]
    pub fn protocol_len(&self) -> u8 {
        read_u8(self.bytes, 5, "arp.truncated").unwrap_or(0)
    }

    /// Operation code: 1 request, 2 reply, other values are ignored by the interface.
    #[must_use]
    pub fn operation(&self) -> u16 {
        read_u16(self.bytes, 6, "arp.truncated").unwrap_or(0)
    }

    /// Sender hardware address.
    #[must_use]
    pub fn sender_hardware(&self) -> MacAddr {
        mac_at(self.bytes, 8)
    }

    /// Sender protocol (IPv4) address.
    #[must_use]
    pub fn sender_protocol(&self) -> Ipv4Addr {
        ipv4_at(self.bytes, 14)
    }

    /// Target hardware address.
    #[must_use]
    pub fn target_hardware(&self) -> MacAddr {
        mac_at(self.bytes, 18)
    }

    /// Target protocol (IPv4) address.
    #[must_use]
    pub fn target_protocol(&self) -> Ipv4Addr {
        ipv4_at(self.bytes, 24)
    }

    /// The complete packet bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

fn mac_at(bytes: &[u8], offset: usize) -> MacAddr {
    let mut octets = [0u8; 6];
    if let Some(part) = bytes.get(offset..offset.saturating_add(6)) {
        octets.copy_from_slice(part);
    }
    MacAddr::new(octets)
}

fn ipv4_at(bytes: &[u8], offset: usize) -> Ipv4Addr {
    let mut octets = [0u8; 4];
    if let Some(part) = bytes.get(offset..offset.saturating_add(4)) {
        octets.copy_from_slice(part);
    }
    Ipv4Addr::new(octets)
}

/// Builder for an Ethernet/IPv4 ARP packet.
#[derive(Debug)]
pub struct ArpBuilder<'a> {
    buffer: &'a mut [u8],
}

impl<'a> ArpBuilder<'a> {
    /// Write an ARP packet into `buffer`.
    ///
    /// # Errors
    /// Returns `arp.truncated` when the buffer is shorter than [`PACKET_LEN`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        buffer: &'a mut [u8],
        operation: u16,
        sender_hardware: MacAddr,
        sender_protocol: Ipv4Addr,
        target_hardware: MacAddr,
        target_protocol: Ipv4Addr,
    ) -> Result<Self, ParseError> {
        let Some(packet) = buffer.get_mut(..PACKET_LEN) else {
            return Err(ParseError::Truncated {
                tag: "arp.truncated",
                need: PACKET_LEN,
                got: buffer.len(),
            });
        };
        packet[0..2].copy_from_slice(&1u16.to_be_bytes());
        packet[2..4].copy_from_slice(&0x0800u16.to_be_bytes());
        packet[4] = 6;
        packet[5] = 4;
        packet[6..8].copy_from_slice(&operation.to_be_bytes());
        packet[8..14].copy_from_slice(&sender_hardware.octets());
        packet[14..18].copy_from_slice(&sender_protocol.octets());
        packet[18..24].copy_from_slice(&target_hardware.octets());
        packet[24..28].copy_from_slice(&target_protocol.octets());
        Ok(Self { buffer })
    }

    /// Finish the packet and return the written bytes.
    pub fn finish(self) -> &'a mut [u8] {
        match self.buffer.get_mut(..PACKET_LEN) {
            Some(packet) => packet,
            None => &mut [],
        }
    }
}
