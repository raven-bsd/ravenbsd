// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! UDP datagram parsing and construction (RFC 768).

use crate::checksum::ChecksumContext;
use crate::{ParseError, read_u16};

/// Length of a UDP header.
pub const HEADER_LEN: usize = 8;

/// A borrowed UDP datagram.
#[derive(Debug, Clone, Copy)]
pub struct UdpPacket<'a> {
    bytes: &'a [u8],
    length: u16,
}

impl<'a> UdpPacket<'a> {
    /// Parse and validate a UDP datagram against `context`.
    ///
    /// Validation order is pinned by `spec/tcpip/vectors/headers.md`: truncation, length, then
    /// checksum. A zero checksum is accepted for IPv4 and rejected for IPv6.
    ///
    /// # Errors
    /// Returns `udp.truncated`, `udp.length`, `udp.zero_checksum_ipv6` or `udp.checksum`.
    pub fn parse(bytes: &'a [u8], context: ChecksumContext) -> Result<Self, ParseError> {
        if bytes.len() < HEADER_LEN {
            return Err(ParseError::Truncated {
                tag: "udp.truncated",
                need: HEADER_LEN,
                got: bytes.len(),
            });
        }
        let length = read_u16(bytes, 4, "udp.truncated")?;
        let length_usize = usize::from(length);
        if length < 8 || length_usize > bytes.len() {
            return Err(ParseError::Malformed { tag: "udp.length" });
        }
        let segment = bytes.get(..length_usize).unwrap_or(bytes);
        let checksum = read_u16(segment, 6, "udp.truncated")?;
        if context.is_ipv6() && checksum == 0 {
            return Err(ParseError::Malformed {
                tag: "udp.zero_checksum_ipv6",
            });
        }
        if checksum != 0 && !context.verify(17, segment) {
            return Err(ParseError::BadChecksum {
                tag: "udp.checksum",
            });
        }
        Ok(Self {
            bytes: segment,
            length,
        })
    }

    /// Source port.
    #[must_use]
    pub fn src_port(&self) -> u16 {
        read_u16(self.bytes, 0, "udp.truncated").unwrap_or(0)
    }

    /// Destination port.
    #[must_use]
    pub fn dst_port(&self) -> u16 {
        read_u16(self.bytes, 2, "udp.truncated").unwrap_or(0)
    }

    /// UDP length from the wire, including the eight-byte header.
    #[must_use]
    pub const fn length(&self) -> u16 {
        self.length
    }

    /// Checksum as read from the wire (zero means "not computed" on IPv4).
    #[must_use]
    pub fn checksum(&self) -> u16 {
        read_u16(self.bytes, 6, "udp.truncated").unwrap_or(0)
    }

    /// The datagram payload.
    #[must_use]
    pub fn payload(&self) -> &'a [u8] {
        self.bytes.get(HEADER_LEN..).unwrap_or(&[])
    }

    /// Length of the datagram payload.
    #[must_use]
    pub fn payload_len(&self) -> usize {
        usize::from(self.length).saturating_sub(HEADER_LEN)
    }

    /// The complete datagram bytes, trimmed to the wire length.
    #[must_use]
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

/// Builder for a UDP datagram.
#[derive(Debug)]
pub struct UdpBuilder<'a> {
    buffer: &'a mut [u8],
    total_len: usize,
    context: ChecksumContext,
}

impl<'a> UdpBuilder<'a> {
    /// Write a UDP header for `payload_len` octets and compute the checksum against `context`.
    ///
    /// # Errors
    /// Returns `udp.length` when the datagram does not fit the 16-bit length field and
    /// `udp.truncated` when the buffer is too short.
    pub fn new(
        buffer: &'a mut [u8],
        context: ChecksumContext,
        src_port: u16,
        dst_port: u16,
        payload_len: usize,
    ) -> Result<Self, ParseError> {
        let total_len = HEADER_LEN
            .checked_add(payload_len)
            .ok_or(ParseError::Malformed { tag: "udp.length" })?;
        if total_len > usize::from(u16::MAX) {
            return Err(ParseError::Malformed { tag: "udp.length" });
        }
        let Some(segment) = buffer.get_mut(..total_len) else {
            return Err(ParseError::Truncated {
                tag: "udp.truncated",
                need: total_len,
                got: buffer.len(),
            });
        };
        let short_len =
            u16::try_from(total_len).map_err(|_| ParseError::Malformed { tag: "udp.length" })?;
        segment[0..2].copy_from_slice(&src_port.to_be_bytes());
        segment[2..4].copy_from_slice(&dst_port.to_be_bytes());
        segment[4..6].copy_from_slice(&short_len.to_be_bytes());
        segment[6] = 0;
        segment[7] = 0;
        Ok(Self {
            buffer,
            total_len,
            context,
        })
    }

    /// The datagram payload area.
    pub fn payload_mut(&mut self) -> &mut [u8] {
        match self.buffer.get_mut(HEADER_LEN..self.total_len) {
            Some(payload) => payload,
            None => &mut [],
        }
    }

    /// Finish the datagram, compute its checksum and return the written bytes.
    pub fn finish(self) -> &'a mut [u8] {
        let Self {
            buffer,
            total_len,
            context,
        } = self;
        let Some(segment) = buffer.get_mut(..total_len) else {
            return &mut [];
        };
        if let Some(field) = segment.get_mut(6..8) {
            field.copy_from_slice(&[0, 0]);
        }
        let checksum = context.compute(17, segment);
        if let Some(field) = segment.get_mut(6..8) {
            field.copy_from_slice(&checksum.to_be_bytes());
        }
        segment
    }
}
