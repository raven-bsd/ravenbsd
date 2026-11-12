// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ICMPv6 message parsing and construction (RFC 4443).

use crate::checksum::ChecksumContext;
use crate::ipv6::Ipv6Addr;
use crate::{ParseError, read_u8, read_u16, read_u32};

/// Length of an ICMPv6 header.
pub const HEADER_LEN: usize = 8;

/// A borrowed ICMPv6 message.
#[derive(Debug, Clone, Copy)]
pub struct Icmpv6Packet<'a> {
    bytes: &'a [u8],
}

impl<'a> Icmpv6Packet<'a> {
    /// Parse and validate an ICMPv6 message against its IPv6 pseudo-header.
    ///
    /// Unknown types parse successfully; the dispatcher decides whether to act.
    ///
    /// # Errors
    /// Returns `icmpv6.truncated` or `icmpv6.checksum`.
    pub fn parse(bytes: &'a [u8], context: ChecksumContext) -> Result<Self, ParseError> {
        if bytes.len() < HEADER_LEN {
            return Err(ParseError::Truncated {
                tag: "icmpv6.truncated",
                need: HEADER_LEN,
                got: bytes.len(),
            });
        }
        if !context.verify(58, bytes) {
            return Err(ParseError::BadChecksum {
                tag: "icmpv6.checksum",
            });
        }
        Ok(Self { bytes })
    }

    /// Message type.
    #[must_use]
    pub fn message_type(&self) -> u8 {
        read_u8(self.bytes, 0, "icmpv6.truncated").unwrap_or(0)
    }

    /// Message code.
    #[must_use]
    pub fn code(&self) -> u8 {
        read_u8(self.bytes, 1, "icmpv6.truncated").unwrap_or(0)
    }

    /// Checksum as read from the wire.
    #[must_use]
    pub fn checksum(&self) -> u16 {
        read_u16(self.bytes, 2, "icmpv6.truncated").unwrap_or(0)
    }

    /// The 32-bit rest-of-header field.
    #[must_use]
    pub fn rest(&self) -> u32 {
        read_u32(self.bytes, 4, "icmpv6.truncated").unwrap_or(0)
    }

    /// Echo identifier (high 16 bits of `rest`).
    #[must_use]
    pub fn id(&self) -> u16 {
        match self.rest().to_be_bytes() {
            [hi, lo, _, _] => u16::from_be_bytes([hi, lo]),
        }
    }

    /// Echo sequence number (low 16 bits of `rest`).
    #[must_use]
    pub fn sequence(&self) -> u16 {
        match self.rest().to_be_bytes() {
            [_, _, hi, lo] => u16::from_be_bytes([hi, lo]),
        }
    }

    /// MTU for a packet-too-big message (type 2).
    #[must_use]
    pub fn mtu(&self) -> Option<u32> {
        if self.message_type() == 2 {
            Some(self.rest())
        } else {
            None
        }
    }

    /// Target address for a neighbour solicitation (135) or advertisement (136).
    #[must_use]
    pub fn target(&self) -> Option<Ipv6Addr> {
        if self.message_type() == 135 || self.message_type() == 136 {
            Ipv6Addr::from_bytes(self.bytes.get(HEADER_LEN..HEADER_LEN.saturating_add(16))?)
        } else {
            None
        }
    }

    /// The message body after the eight-byte header.
    #[must_use]
    pub fn payload(&self) -> &'a [u8] {
        self.bytes.get(HEADER_LEN..).unwrap_or(&[])
    }

    /// Length of the message body.
    #[must_use]
    pub fn payload_len(&self) -> usize {
        self.bytes.len().saturating_sub(HEADER_LEN)
    }

    /// The complete message bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

/// Builder for an ICMPv6 message.
#[derive(Debug)]
pub struct Icmpv6Builder<'a> {
    buffer: &'a mut [u8],
    total_len: usize,
    context: ChecksumContext,
}

impl<'a> Icmpv6Builder<'a> {
    /// Write an ICMPv6 message and compute its checksum against `context`.
    ///
    /// # Errors
    /// Returns `icmpv6.truncated` when the buffer cannot hold the message.
    pub fn new(
        buffer: &'a mut [u8],
        context: ChecksumContext,
        message_type: u8,
        code: u8,
        rest: u32,
        payload_len: usize,
    ) -> Result<Self, ParseError> {
        let total_len = HEADER_LEN
            .checked_add(payload_len)
            .ok_or(ParseError::Truncated {
                tag: "icmpv6.truncated",
                need: usize::MAX,
                got: buffer.len(),
            })?;
        let Some(message) = buffer.get_mut(..total_len) else {
            return Err(ParseError::Truncated {
                tag: "icmpv6.truncated",
                need: total_len,
                got: buffer.len(),
            });
        };
        message[0] = message_type;
        message[1] = code;
        message[2] = 0;
        message[3] = 0;
        message[4..8].copy_from_slice(&rest.to_be_bytes());
        Ok(Self {
            buffer,
            total_len,
            context,
        })
    }

    /// The message body area.
    pub fn payload_mut(&mut self) -> &mut [u8] {
        match self.buffer.get_mut(HEADER_LEN..self.total_len) {
            Some(payload) => payload,
            None => &mut [],
        }
    }

    /// Finish the message, compute its checksum and return the written bytes.
    pub fn finish(self) -> &'a mut [u8] {
        let Self {
            buffer,
            total_len,
            context,
        } = self;
        let Some(message) = buffer.get_mut(..total_len) else {
            return &mut [];
        };
        if let Some(field) = message.get_mut(2..4) {
            field.copy_from_slice(&[0, 0]);
        }
        let checksum = context.compute(58, message);
        if let Some(field) = message.get_mut(2..4) {
            field.copy_from_slice(&checksum.to_be_bytes());
        }
        message
    }
}
