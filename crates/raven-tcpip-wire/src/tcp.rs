// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TCP segment parsing, options and construction (RFC 9293, RFC 7323, RFC 2018).

use core::fmt;

use crate::checksum::ChecksumContext;
use crate::{ParseError, read_u8, read_u16, read_u32};

/// Length of a minimal TCP header.
pub const MIN_HEADER_LEN: usize = 20;

/// The TCP control bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TcpFlags(u8);

impl TcpFlags {
    /// Congestion window reduced.
    pub const CWR: Self = Self(0x80);
    /// ECN echo.
    pub const ECE: Self = Self(0x40);
    /// Urgent pointer valid.
    pub const URG: Self = Self(0x20);
    /// Acknowledgement valid.
    pub const ACK: Self = Self(0x10);
    /// Push.
    pub const PSH: Self = Self(0x08);
    /// Reset.
    pub const RST: Self = Self(0x04);
    /// Synchronise sequence numbers.
    pub const SYN: Self = Self(0x02);
    /// Finish.
    pub const FIN: Self = Self(0x01);

    /// Construct from the low eight bits of `bits`.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// Return the raw flag byte.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Whether CWR is set.
    #[must_use]
    pub const fn cwr(self) -> bool {
        self.0 & 0x80 != 0
    }

    /// Whether ECE is set.
    #[must_use]
    pub const fn ece(self) -> bool {
        self.0 & 0x40 != 0
    }

    /// Whether URG is set.
    #[must_use]
    pub const fn urg(self) -> bool {
        self.0 & 0x20 != 0
    }

    /// Whether ACK is set.
    #[must_use]
    pub const fn ack(self) -> bool {
        self.0 & 0x10 != 0
    }

    /// Whether PSH is set.
    #[must_use]
    pub const fn psh(self) -> bool {
        self.0 & 0x08 != 0
    }

    /// Whether RST is set.
    #[must_use]
    pub const fn rst(self) -> bool {
        self.0 & 0x04 != 0
    }

    /// Whether SYN is set.
    #[must_use]
    pub const fn syn(self) -> bool {
        self.0 & 0x02 != 0
    }

    /// Whether FIN is set.
    #[must_use]
    pub const fn fin(self) -> bool {
        self.0 & 0x01 != 0
    }

    /// Set or clear a flag by its mask.
    pub fn set(&mut self, mask: Self, enabled: bool) {
        if enabled {
            self.0 |= mask.0;
        } else {
            self.0 &= !mask.0;
        }
    }
}

impl fmt::Display for TcpFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for (set, name) in [
            (self.cwr(), "CWR"),
            (self.ece(), "ECE"),
            (self.urg(), "URG"),
            (self.ack(), "ACK"),
            (self.psh(), "PSH"),
            (self.rst(), "RST"),
            (self.syn(), "SYN"),
            (self.fin(), "FIN"),
        ] {
            if set {
                if !first {
                    f.write_str(",")?;
                }
                f.write_str(name)?;
                first = false;
            }
        }
        Ok(())
    }
}

/// A borrowed TCP segment.
#[derive(Debug, Clone, Copy)]
pub struct TcpPacket<'a> {
    bytes: &'a [u8],
    header_len: usize,
}

impl<'a> TcpPacket<'a> {
    /// Parse and validate a TCP segment against `context`.
    ///
    /// Validation order is pinned by `spec/tcpip/vectors/headers.md`: truncation, data offset,
    /// every option, then checksum.
    ///
    /// # Errors
    /// Returns `tcp.truncated`, `tcp.data_offset`, `tcp.option_length`,
    /// `tcp.option_truncated` or `tcp.checksum`.
    pub fn parse(bytes: &'a [u8], context: ChecksumContext) -> Result<Self, ParseError> {
        if bytes.len() < MIN_HEADER_LEN {
            return Err(ParseError::Truncated {
                tag: "tcp.truncated",
                need: MIN_HEADER_LEN,
                got: bytes.len(),
            });
        }
        let data_offset = read_u8(bytes, 12, "tcp.truncated")? >> 4;
        let header_len = usize::from(data_offset) * 4;
        if data_offset < 5 || header_len > bytes.len() {
            return Err(ParseError::BadHeaderLen {
                tag: "tcp.data_offset",
                value: data_offset,
            });
        }
        let option_area = bytes.get(MIN_HEADER_LEN..header_len).unwrap_or(&[]);
        for option in TcpOptions::new(option_area) {
            option?;
        }
        if !context.verify(6, bytes) {
            return Err(ParseError::BadChecksum {
                tag: "tcp.checksum",
            });
        }
        Ok(Self { bytes, header_len })
    }

    /// Source port.
    #[must_use]
    pub fn src_port(&self) -> u16 {
        read_u16(self.bytes, 0, "tcp.truncated").unwrap_or(0)
    }

    /// Destination port.
    #[must_use]
    pub fn dst_port(&self) -> u16 {
        read_u16(self.bytes, 2, "tcp.truncated").unwrap_or(0)
    }

    /// Sequence number.
    #[must_use]
    pub fn sequence(&self) -> u32 {
        read_u32(self.bytes, 4, "tcp.truncated").unwrap_or(0)
    }

    /// Acknowledgement number.
    #[must_use]
    pub fn acknowledgement(&self) -> u32 {
        read_u32(self.bytes, 8, "tcp.truncated").unwrap_or(0)
    }

    /// Data offset in 32-bit words.
    #[must_use]
    pub fn data_offset(&self) -> u8 {
        match self.bytes.get(12) {
            Some(&byte) => byte >> 4,
            None => 0,
        }
    }

    /// Control bits.
    #[must_use]
    pub fn flags(&self) -> TcpFlags {
        match self.bytes.get(13) {
            Some(&byte) => TcpFlags::from_bits(byte),
            None => TcpFlags::default(),
        }
    }

    /// Raw (unscaled) window.
    #[must_use]
    pub fn window(&self) -> u16 {
        read_u16(self.bytes, 14, "tcp.truncated").unwrap_or(0)
    }

    /// Checksum as read from the wire.
    #[must_use]
    pub fn checksum(&self) -> u16 {
        read_u16(self.bytes, 16, "tcp.truncated").unwrap_or(0)
    }

    /// Urgent pointer.
    #[must_use]
    pub fn urgent_pointer(&self) -> u16 {
        read_u16(self.bytes, 18, "tcp.truncated").unwrap_or(0)
    }

    /// Header length in bytes, including options.
    #[must_use]
    pub const fn header_len(&self) -> usize {
        self.header_len
    }

    /// Iterate over the options in wire order.
    #[must_use]
    pub fn options(&self) -> TcpOptions<'a> {
        TcpOptions::new(
            self.bytes
                .get(MIN_HEADER_LEN..self.header_len)
                .unwrap_or(&[]),
        )
    }

    /// The segment payload.
    #[must_use]
    pub fn payload(&self) -> &'a [u8] {
        self.bytes.get(self.header_len..).unwrap_or(&[])
    }

    /// Length of the segment payload.
    #[must_use]
    pub fn payload_len(&self) -> usize {
        self.bytes.len().saturating_sub(self.header_len)
    }

    /// The complete segment bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

/// A parsed TCP option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpOption<'a> {
    /// End of option list (kind 0).
    EndOfList,
    /// No-operation padding (kind 1).
    NoOperation,
    /// Maximum segment size (kind 2).
    MaxSegmentSize(u16),
    /// Window scale shift count (kind 3).
    WindowScale(u8),
    /// SACK-permitted (kind 4).
    SackPermitted,
    /// SACK blocks (kind 5).
    Sack(SackBlocks<'a>),
    /// Timestamps (kind 8).
    Timestamps {
        /// Timestamp value.
        tsval: u32,
        /// Timestamp echo reply.
        tsecr: u32,
    },
    /// TCP MD5 signature option (kind 19), digest bytes only.
    Md5 {
        /// Digest bytes as carried on the wire (normally 16).
        digest: &'a [u8],
    },
    /// TCP authentication option (kind 29), body bytes only.
    Authentication {
        /// Key id, next key id and MAC, as carried after the length octet.
        data: &'a [u8],
    },
    /// Any other option.
    Unknown {
        /// Option kind.
        kind: u8,
        /// Option body after the length octet.
        data: &'a [u8],
    },
}

/// Iterator over TCP options.
///
/// The iterator terminates on every input. A malformed option yields one `Err` and then ends.
#[derive(Debug, Clone)]
pub struct TcpOptions<'a> {
    bytes: &'a [u8],
    offset: usize,
    failed: bool,
}

impl<'a> TcpOptions<'a> {
    /// Iterate over an option area.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            offset: 0,
            failed: false,
        }
    }
}

impl<'a> Iterator for TcpOptions<'a> {
    type Item = Result<TcpOption<'a>, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        let remaining = self.bytes.get(self.offset..)?;
        let &kind = remaining.first()?;
        match kind {
            0 => {
                self.offset = self.offset.saturating_add(1);
                Some(Ok(TcpOption::EndOfList))
            }
            1 => {
                self.offset = self.offset.saturating_add(1);
                Some(Ok(TcpOption::NoOperation))
            }
            _ => {
                let Some(&length_byte) = remaining.get(1) else {
                    self.failed = true;
                    return Some(Err(ParseError::BadOption {
                        tag: "tcp.option_truncated",
                        offset: self.offset,
                    }));
                };
                let length = usize::from(length_byte);
                if length < 2 {
                    self.failed = true;
                    return Some(Err(ParseError::BadOption {
                        tag: "tcp.option_length",
                        offset: self.offset,
                    }));
                }
                if length > remaining.len() {
                    self.failed = true;
                    return Some(Err(ParseError::BadOption {
                        tag: "tcp.option_truncated",
                        offset: self.offset,
                    }));
                }
                let data = remaining.get(2..length).unwrap_or(&[]);
                let exact = match kind {
                    2 => length == 4,
                    3 => length == 3,
                    4 => length == 2,
                    5 => length >= 10 && (length - 2) % 8 == 0,
                    8 => length == 10,
                    _ => true,
                };
                if !exact {
                    self.failed = true;
                    return Some(Err(ParseError::BadOption {
                        tag: "tcp.option_length",
                        offset: self.offset,
                    }));
                }
                self.offset = self.offset.saturating_add(length);
                let option = match kind {
                    2 => match data {
                        &[hi, lo] => TcpOption::MaxSegmentSize(u16::from_be_bytes([hi, lo])),
                        _ => TcpOption::Unknown { kind, data },
                    },
                    3 => match data {
                        &[shift] => TcpOption::WindowScale(shift),
                        _ => TcpOption::Unknown { kind, data },
                    },
                    4 => TcpOption::SackPermitted,
                    5 => TcpOption::Sack(SackBlocks::new(data)),
                    8 => match data {
                        &[a, b, c, d, e, f, g, h] => TcpOption::Timestamps {
                            tsval: u32::from_be_bytes([a, b, c, d]),
                            tsecr: u32::from_be_bytes([e, f, g, h]),
                        },
                        _ => TcpOption::Unknown { kind, data },
                    },
                    19 => TcpOption::Md5 { digest: data },
                    29 => TcpOption::Authentication { data },
                    _ => TcpOption::Unknown { kind, data },
                };
                Some(Ok(option))
            }
        }
    }
}

/// Iterator over SACK blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SackBlocks<'a> {
    bytes: &'a [u8],
}

impl<'a> SackBlocks<'a> {
    /// Construct from the option body (eight bytes per block).
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// Number of blocks.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len() / 8
    }

    /// Whether there are no blocks.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl Iterator for SackBlocks<'_> {
    type Item = (u32, u32);

    fn next(&mut self) -> Option<Self::Item> {
        let (block, rest) = self.bytes.split_at_checked(8)?;
        self.bytes = rest;
        match block {
            [a, b, c, d, e, f, g, h] => Some((
                u32::from_be_bytes([*a, *b, *c, *d]),
                u32::from_be_bytes([*e, *f, *g, *h]),
            )),
            _ => None,
        }
    }
}

/// The fields of a TCP header that a sender controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TcpHeader {
    /// Source port.
    pub src_port: u16,
    /// Destination port.
    pub dst_port: u16,
    /// Sequence number.
    pub sequence: u32,
    /// Acknowledgement number.
    pub acknowledgement: u32,
    /// Control bits.
    pub flags: TcpFlags,
    /// Raw (unscaled) window.
    pub window: u16,
    /// Urgent pointer.
    pub urgent_pointer: u16,
}

/// Builder for a TCP segment.
#[derive(Debug)]
pub struct TcpBuilder<'a> {
    buffer: &'a mut [u8],
    header_len: usize,
    total_len: usize,
    context: ChecksumContext,
}

impl<'a> TcpBuilder<'a> {
    /// Write a TCP header with `options` and `payload_len` payload octets, then compute the
    /// checksum against `context`.
    ///
    /// Options are padded with zero octets (EOL) to a four-byte boundary.
    ///
    /// # Errors
    /// Returns `tcp.data_offset` when the header would exceed 60 bytes, `tcp.truncated` when the
    /// buffer is too short, and `tcp.truncated` when the segment is longer than an IP payload.
    pub fn new(
        buffer: &'a mut [u8],
        context: ChecksumContext,
        header: &TcpHeader,
        options: &[u8],
        payload_len: usize,
    ) -> Result<Self, ParseError> {
        let option_len = options.len();
        let unpadded = MIN_HEADER_LEN
            .checked_add(option_len)
            .ok_or(ParseError::Malformed {
                tag: "tcp.data_offset",
            })?;
        let header_len =
            unpadded
                .checked_add(3)
                .map(|value| value & !3)
                .ok_or(ParseError::Malformed {
                    tag: "tcp.data_offset",
                })?;
        if header_len > 60 {
            return Err(ParseError::Malformed {
                tag: "tcp.data_offset",
            });
        }
        let total_len = header_len
            .checked_add(payload_len)
            .ok_or(ParseError::Truncated {
                tag: "tcp.truncated",
                need: usize::MAX,
                got: buffer.len(),
            })?;
        if total_len > 0xffff {
            return Err(ParseError::Truncated {
                tag: "tcp.truncated",
                need: total_len,
                got: buffer.len(),
            });
        }
        let Some(segment) = buffer.get_mut(..total_len) else {
            return Err(ParseError::Truncated {
                tag: "tcp.truncated",
                need: total_len,
                got: buffer.len(),
            });
        };
        segment[0..2].copy_from_slice(&header.src_port.to_be_bytes());
        segment[2..4].copy_from_slice(&header.dst_port.to_be_bytes());
        segment[4..8].copy_from_slice(&header.sequence.to_be_bytes());
        segment[8..12].copy_from_slice(&header.acknowledgement.to_be_bytes());
        let data_offset = u8::try_from(header_len / 4).map_err(|_| ParseError::Malformed {
            tag: "tcp.data_offset",
        })?;
        segment[12] = data_offset << 4;
        segment[13] = header.flags.bits();
        segment[14..16].copy_from_slice(&header.window.to_be_bytes());
        segment[16] = 0;
        segment[17] = 0;
        segment[18..20].copy_from_slice(&header.urgent_pointer.to_be_bytes());
        if let Some(option_area) = segment.get_mut(MIN_HEADER_LEN..header_len) {
            option_area.fill(0);
            if let Some(destination) = option_area.get_mut(..option_len) {
                destination.copy_from_slice(options);
            }
        }
        Ok(Self {
            buffer,
            header_len,
            total_len,
            context,
        })
    }

    /// The header length in bytes, including padding.
    #[must_use]
    pub const fn header_len(&self) -> usize {
        self.header_len
    }

    /// The segment payload area.
    pub fn payload_mut(&mut self) -> &mut [u8] {
        match self.buffer.get_mut(self.header_len..self.total_len) {
            Some(payload) => payload,
            None => &mut [],
        }
    }

    /// Finish the segment, compute its checksum and return the written bytes.
    pub fn finish(self) -> &'a mut [u8] {
        let Self {
            buffer,
            header_len: _,
            total_len,
            context,
        } = self;
        let Some(segment) = buffer.get_mut(..total_len) else {
            return &mut [];
        };
        if let Some(field) = segment.get_mut(16..18) {
            field.copy_from_slice(&[0, 0]);
        }
        let checksum = context.compute(6, segment);
        if let Some(field) = segment.get_mut(16..18) {
            field.copy_from_slice(&checksum.to_be_bytes());
        }
        segment
    }
}
