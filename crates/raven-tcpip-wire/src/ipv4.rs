// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! IPv4 header parsing and construction (RFC 791).

use core::fmt;

use crate::checksum::internet_checksum;
use crate::{ParseError, read_u8, read_u16, slice, write_u16};

/// Length of a minimal IPv4 header.
pub const MIN_HEADER_LEN: usize = 20;

/// An IPv4 address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Ipv4Addr([u8; 4]);

impl Ipv4Addr {
    /// The unspecified address `0.0.0.0`.
    pub const UNSPECIFIED: Self = Self([0; 4]);
    /// The limited broadcast address `255.255.255.255`.
    pub const BROADCAST: Self = Self([0xff; 4]);

    /// Construct an address from its four octets.
    #[must_use]
    pub const fn new(octets: [u8; 4]) -> Self {
        Self(octets)
    }

    /// Return the four octets in wire order.
    #[must_use]
    pub const fn octets(self) -> [u8; 4] {
        self.0
    }

    /// Construct an address from four octets if the slice is exactly that long.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes {
            &[a, b, c, d] => Some(Self([a, b, c, d])),
            _ => None,
        }
    }

    /// Whether this is the unspecified address.
    #[must_use]
    pub const fn is_unspecified(self) -> bool {
        matches!(self.0, [0, 0, 0, 0])
    }

    /// Whether this is the limited broadcast address.
    #[must_use]
    pub const fn is_broadcast(self) -> bool {
        matches!(self.0, [0xff, 0xff, 0xff, 0xff])
    }

    /// Whether this is a multicast address (`224.0.0.0/4`).
    #[must_use]
    pub const fn is_multicast(self) -> bool {
        self.0[0] & 0xf0 == 0xe0
    }
}

impl From<[u8; 4]> for Ipv4Addr {
    fn from(octets: [u8; 4]) -> Self {
        Self(octets)
    }
}

impl fmt::Display for Ipv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.0;
        write!(f, "{a}.{b}.{c}.{d}")
    }
}

/// The three IPv4 flag bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Ipv4Flags(u8);

impl Ipv4Flags {
    /// Don't-fragment.
    pub const DF: Self = Self(0b010);
    /// More-fragments.
    pub const MF: Self = Self(0b001);
    /// The reserved bit.
    pub const RESERVED: Self = Self(0b100);

    /// Construct from the low three bits of `bits`.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 0b111)
    }

    /// Return the low three bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Whether the don't-fragment bit is set.
    #[must_use]
    pub const fn df(self) -> bool {
        self.0 & 0b010 != 0
    }

    /// Whether the more-fragments bit is set.
    #[must_use]
    pub const fn mf(self) -> bool {
        self.0 & 0b001 != 0
    }

    /// Whether the reserved bit is set.
    #[must_use]
    pub const fn reserved(self) -> bool {
        self.0 & 0b100 != 0
    }

    /// Set or clear the don't-fragment bit.
    pub fn set_df(&mut self, enabled: bool) {
        if enabled {
            self.0 |= 0b010;
        } else {
            self.0 &= !0b010;
        }
    }

    /// Set or clear the more-fragments bit.
    pub fn set_mf(&mut self, enabled: bool) {
        if enabled {
            self.0 |= 0b001;
        } else {
            self.0 &= !0b001;
        }
    }
}

impl fmt::Display for Ipv4Flags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        if self.df() {
            f.write_str("DF")?;
            first = false;
        }
        if self.mf() {
            if !first {
                f.write_str(",")?;
            }
            f.write_str("MF")?;
        }
        Ok(())
    }
}

/// A borrowed IPv4 packet.
#[derive(Debug, Clone, Copy)]
pub struct Ipv4Packet<'a> {
    bytes: &'a [u8],
    header_len: usize,
    total_len: usize,
}

impl<'a> Ipv4Packet<'a> {
    /// Parse and validate an IPv4 packet.
    ///
    /// Validation order is pinned by `spec/tcpip/vectors/headers.md`: truncation, version, IHL,
    /// total length, then header checksum.
    ///
    /// # Errors
    /// Returns `ipv4.truncated`, `ipv4.version`, `ipv4.ihl`, `ipv4.total_len` or
    /// `ipv4.checksum`.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < MIN_HEADER_LEN {
            return Err(ParseError::Truncated {
                tag: "ipv4.truncated",
                need: MIN_HEADER_LEN,
                got: bytes.len(),
            });
        }
        let first = read_u8(bytes, 0, "ipv4.truncated")?;
        let version = first >> 4;
        if version != 4 {
            return Err(ParseError::BadVersion {
                tag: "ipv4.version",
                version,
            });
        }
        let ihl = first & 0x0f;
        let header_len = usize::from(ihl) * 4;
        if ihl < 5 || header_len > bytes.len() {
            return Err(ParseError::BadHeaderLen {
                tag: "ipv4.ihl",
                value: ihl,
            });
        }
        let total_len = usize::from(read_u16(bytes, 2, "ipv4.truncated")?);
        if total_len < header_len || total_len > bytes.len() {
            return Err(ParseError::Malformed {
                tag: "ipv4.total_len",
            });
        }
        let header = slice(bytes, 0, header_len, "ipv4.truncated")?;
        if internet_checksum(&[header]) != 0 {
            return Err(ParseError::BadChecksum {
                tag: "ipv4.checksum",
            });
        }
        Ok(Self {
            bytes,
            header_len,
            total_len,
        })
    }

    /// IP version (always 4 after a successful parse).
    #[must_use]
    pub fn version(&self) -> u8 {
        match self.bytes.first() {
            Some(first) => first >> 4,
            None => 0,
        }
    }

    /// Internet header length in 32-bit words.
    #[must_use]
    pub fn ihl(&self) -> u8 {
        match self.bytes.first() {
            Some(first) => first & 0x0f,
            None => 0,
        }
    }

    /// Differentiated services code point.
    #[must_use]
    pub fn dscp(&self) -> u8 {
        self.second_byte() >> 2
    }

    /// Explicit congestion notification bits.
    #[must_use]
    pub fn ecn(&self) -> u8 {
        self.second_byte() & 0x03
    }

    fn second_byte(&self) -> u8 {
        match self.bytes.get(1) {
            Some(&byte) => byte,
            None => 0,
        }
    }

    /// Total length from the wire, including the header.
    #[must_use]
    pub fn total_len(&self) -> u16 {
        read_u16(self.bytes, 2, "ipv4.truncated").unwrap_or(0)
    }

    /// Identification field.
    #[must_use]
    pub fn id(&self) -> u16 {
        read_u16(self.bytes, 4, "ipv4.truncated").unwrap_or(0)
    }

    /// Flag bits.
    #[must_use]
    pub fn flags(&self) -> Ipv4Flags {
        match self.bytes.get(6) {
            Some(&byte) => Ipv4Flags::from_bits(byte >> 5),
            None => Ipv4Flags::default(),
        }
    }

    /// Fragment offset in bytes.
    #[must_use]
    pub fn frag_offset(&self) -> u16 {
        let units = match self.bytes.get(6..8) {
            Some(&[hi, lo]) => u16::from_be_bytes([hi & 0x1f, lo]),
            _ => 0,
        };
        units.saturating_mul(8)
    }

    /// Time to live.
    #[must_use]
    pub fn ttl(&self) -> u8 {
        match self.bytes.get(8) {
            Some(&byte) => byte,
            None => 0,
        }
    }

    /// Upper-layer protocol number.
    #[must_use]
    pub fn protocol(&self) -> u8 {
        match self.bytes.get(9) {
            Some(&byte) => byte,
            None => 0,
        }
    }

    /// Header checksum as read from the wire.
    #[must_use]
    pub fn checksum(&self) -> u16 {
        read_u16(self.bytes, 10, "ipv4.truncated").unwrap_or(0)
    }

    /// Source address.
    #[must_use]
    pub fn src(&self) -> Ipv4Addr {
        let mut octets = [0u8; 4];
        if let Some(part) = self.bytes.get(12..16) {
            octets.copy_from_slice(part);
        }
        Ipv4Addr::new(octets)
    }

    /// Destination address.
    #[must_use]
    pub fn dst(&self) -> Ipv4Addr {
        let mut octets = [0u8; 4];
        if let Some(part) = self.bytes.get(16..20) {
            octets.copy_from_slice(part);
        }
        Ipv4Addr::new(octets)
    }

    /// Header length in bytes, including options.
    #[must_use]
    pub const fn header_len(&self) -> usize {
        self.header_len
    }

    /// The option area (`(ihl - 5) * 4` bytes), preserved but not decoded.
    #[must_use]
    pub fn options(&self) -> &'a [u8] {
        self.bytes
            .get(MIN_HEADER_LEN..self.header_len)
            .unwrap_or(&[])
    }

    /// The IP payload, trimmed to the wire total length.
    #[must_use]
    pub fn payload(&self) -> &'a [u8] {
        self.bytes
            .get(self.header_len..self.total_len)
            .unwrap_or(&[])
    }

    /// Length of the payload in bytes.
    #[must_use]
    pub fn payload_len(&self) -> usize {
        self.total_len.saturating_sub(self.header_len)
    }

    /// Whether this packet is a fragment (MF set or a non-zero offset).
    #[must_use]
    pub fn is_fragment(&self) -> bool {
        self.flags().mf() || self.frag_offset() != 0
    }

    /// The complete packet bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

/// The fields of an IPv4 header that a sender controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Header {
    /// Differentiated services code point.
    pub dscp: u8,
    /// Explicit congestion notification bits.
    pub ecn: u8,
    /// Identification field.
    pub id: u16,
    /// Flag bits.
    pub flags: Ipv4Flags,
    /// Fragment offset in bytes; must be a multiple of eight.
    pub frag_offset: u16,
    /// Time to live.
    pub ttl: u8,
    /// Upper-layer protocol number.
    pub protocol: u8,
    /// Source address.
    pub src: Ipv4Addr,
    /// Destination address.
    pub dst: Ipv4Addr,
}

impl Default for Ipv4Header {
    fn default() -> Self {
        Self {
            dscp: 0,
            ecn: 0,
            id: 0,
            flags: Ipv4Flags::default(),
            frag_offset: 0,
            ttl: 64,
            protocol: 0,
            src: Ipv4Addr::UNSPECIFIED,
            dst: Ipv4Addr::UNSPECIFIED,
        }
    }
}

/// Builder for an IPv4 packet with a 20-byte header (no options).
#[derive(Debug)]
pub struct Ipv4Builder<'a> {
    buffer: &'a mut [u8],
    total_len: usize,
}

impl<'a> Ipv4Builder<'a> {
    /// Write an IPv4 header for a payload of `payload_len` octets and compute its checksum.
    ///
    /// # Errors
    /// Returns `ipv4.frag_offset` when the offset is not a multiple of eight,
    /// `ipv4.total_len` when the packet cannot fit in the 16-bit length field or the buffer, or
    /// `ipv4.truncated` when the buffer is too short.
    pub fn new(
        buffer: &'a mut [u8],
        header: &Ipv4Header,
        payload_len: usize,
    ) -> Result<Self, ParseError> {
        if !header.frag_offset.is_multiple_of(8) || header.frag_offset > 0xfff8 {
            return Err(ParseError::Malformed {
                tag: "ipv4.frag_offset",
            });
        }
        let total_len = MIN_HEADER_LEN
            .checked_add(payload_len)
            .ok_or(ParseError::Malformed {
                tag: "ipv4.total_len",
            })?;
        if total_len > usize::from(u16::MAX) {
            return Err(ParseError::Malformed {
                tag: "ipv4.total_len",
            });
        }
        if buffer.len() < total_len {
            return Err(ParseError::Truncated {
                tag: "ipv4.truncated",
                need: total_len,
                got: buffer.len(),
            });
        }
        let Some(header_bytes) = buffer.get_mut(..MIN_HEADER_LEN) else {
            return Err(ParseError::Truncated {
                tag: "ipv4.truncated",
                need: total_len,
                got: buffer.len(),
            });
        };
        let short_total = u16::try_from(total_len).map_err(|_| ParseError::Malformed {
            tag: "ipv4.total_len",
        })?;
        header_bytes[0] = 0x45;
        header_bytes[1] = (header.dscp & 0x3f) << 2 | (header.ecn & 0x03);
        header_bytes[2..4].copy_from_slice(&short_total.to_be_bytes());
        header_bytes[4..6].copy_from_slice(&header.id.to_be_bytes());
        let wire_offset = header.frag_offset / 8;
        header_bytes[6] = (header.flags.bits() & 0x07) << 5 | ((wire_offset >> 8) & 0x1f) as u8;
        header_bytes[7] = (wire_offset & 0xff) as u8;
        header_bytes[8] = header.ttl;
        header_bytes[9] = header.protocol;
        header_bytes[10] = 0;
        header_bytes[11] = 0;
        header_bytes[12..16].copy_from_slice(&header.src.octets());
        header_bytes[16..20].copy_from_slice(&header.dst.octets());
        let checksum = internet_checksum(&[header_bytes]);
        if !write_u16(header_bytes, 10, checksum) {
            return Err(ParseError::Truncated {
                tag: "ipv4.truncated",
                need: MIN_HEADER_LEN,
                got: header_bytes.len(),
            });
        }
        Ok(Self { buffer, total_len })
    }

    /// The payload area.
    pub fn payload_mut(&mut self) -> &mut [u8] {
        match self.buffer.get_mut(MIN_HEADER_LEN..self.total_len) {
            Some(payload) => payload,
            None => &mut [],
        }
    }

    /// Finish the packet and return the written bytes.
    pub fn finish(self) -> &'a mut [u8] {
        match self.buffer.get_mut(..self.total_len) {
            Some(packet) => packet,
            None => &mut [],
        }
    }
}
