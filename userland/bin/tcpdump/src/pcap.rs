// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Classic pcap savefile reading and writing.

use alloc::vec::Vec;

/// `LINKTYPE_ETHERNET` from the pcap link-type registry.
pub const LINKTYPE_ETHERNET: u32 = 1;

const GLOBAL_HEADER_LEN: usize = 24;
const RECORD_HEADER_LEN: usize = 16;
const MAGIC_MICRO: u32 = 0xa1b2_c3d4;
const MAGIC_NANO: u32 = 0xa1b2_3c4d;

/// The fraction precision declared by the savefile magic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampPrecision {
    /// Microseconds (`0xa1b2c3d4`).
    Microseconds,
    /// Nanoseconds (`0xa1b23c4d`).
    Nanoseconds,
}

/// One packet record borrowed from the input image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Packet<'a> {
    /// Seconds since the Unix epoch.
    pub ts_sec: u32,
    /// Fractional seconds in the savefile precision.
    pub ts_frac: u32,
    /// Fraction precision of `ts_frac`.
    pub precision: TimestampPrecision,
    /// Captured length.
    pub caplen: u32,
    /// Original wire length.
    pub orig_len: u32,
    /// Captured frame bytes.
    pub data: &'a [u8],
}

/// A parsed classic-pcap image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcapFile<'a> {
    /// Link type from the global header.
    pub link_type: u32,
    /// Snapshot length from the global header.
    pub snaplen: u32,
    /// Record precision.
    pub precision: TimestampPrecision,
    /// Packet records in file order.
    pub packets: Vec<Packet<'a>>,
}

/// A malformed or unsupported capture file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcapError {
    /// Fewer than 24 global-header bytes.
    TruncatedGlobalHeader,
    /// The magic number is not a supported classic-pcap magic.
    BadMagic([u8; 4]),
    /// The version is not the classic pcap 2.4 family.
    UnsupportedVersion {
        /// Major version.
        major: u16,
        /// Minor version.
        minor: u16,
    },
    /// The link type is not Ethernet.
    UnsupportedLinkType(u32),
    /// A record header or declared frame is incomplete.
    TruncatedFrame {
        /// Byte offset of the record header.
        offset: usize,
        /// Bytes required by the declaration.
        need: usize,
        /// Bytes available.
        got: usize,
    },
    /// The record lengths are inconsistent with the snapshot length.
    BadRecordLengths {
        /// Captured length declared by the record.
        caplen: u32,
        /// Original wire length declared by the record.
        orig_len: u32,
        /// Snapshot length declared by the global header.
        snaplen: u32,
    },
}

impl core::fmt::Display for PcapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TruncatedGlobalHeader => f.write_str("truncated global header"),
            Self::BadMagic(magic) => write!(f, "bad magic {magic:02x?}"),
            Self::UnsupportedVersion { major, minor } => {
                write!(f, "unsupported pcap version {major}.{minor}")
            }
            Self::UnsupportedLinkType(link_type) => {
                write!(f, "unsupported link type {link_type}")
            }
            Self::TruncatedFrame { offset, need, got } => write!(
                f,
                "truncated frame at offset {offset}: need {need} bytes, got {got}"
            ),
            Self::BadRecordLengths {
                caplen,
                orig_len,
                snaplen,
            } => write!(
                f,
                "bad record lengths: caplen {caplen}, orig_len {orig_len}, snaplen {snaplen}"
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Endian {
    Little,
    Big,
}

fn read_u16(bytes: &[u8], offset: usize, endian: Endian) -> Option<u16> {
    let pair = bytes.get(offset..offset.checked_add(2)?)?;
    let pair = [pair[0], pair[1]];
    Some(match endian {
        Endian::Little => u16::from_le_bytes(pair),
        Endian::Big => u16::from_be_bytes(pair),
    })
}

fn read_u32(bytes: &[u8], offset: usize, endian: Endian) -> Option<u32> {
    let word = bytes.get(offset..offset.checked_add(4)?)?;
    let word = [word[0], word[1], word[2], word[3]];
    Some(match endian {
        Endian::Little => u32::from_le_bytes(word),
        Endian::Big => u32::from_be_bytes(word),
    })
}

fn magic(bytes: &[u8]) -> Option<(Endian, TimestampPrecision)> {
    let little = read_u32(bytes, 0, Endian::Little)?;
    match little {
        MAGIC_MICRO => return Some((Endian::Little, TimestampPrecision::Microseconds)),
        MAGIC_NANO => return Some((Endian::Little, TimestampPrecision::Nanoseconds)),
        _ => {}
    }
    let big = read_u32(bytes, 0, Endian::Big)?;
    match big {
        MAGIC_MICRO => Some((Endian::Big, TimestampPrecision::Microseconds)),
        MAGIC_NANO => Some((Endian::Big, TimestampPrecision::Nanoseconds)),
        _ => None,
    }
}

/// Parse a classic-pcap image.
///
/// # Errors
/// Returns [`PcapError`] when the global header, a record header, the declared
/// frame length or the link type is invalid.
pub fn parse(bytes: &[u8]) -> Result<PcapFile<'_>, PcapError> {
    if bytes.len() < GLOBAL_HEADER_LEN {
        return Err(PcapError::TruncatedGlobalHeader);
    }
    let (endian, precision) = magic(bytes).ok_or(PcapError::BadMagic([
        bytes[0], bytes[1], bytes[2], bytes[3],
    ]))?;
    let major = read_u16(bytes, 4, endian).ok_or(PcapError::TruncatedGlobalHeader)?;
    let minor = read_u16(bytes, 6, endian).ok_or(PcapError::TruncatedGlobalHeader)?;
    if major != 2 {
        return Err(PcapError::UnsupportedVersion { major, minor });
    }
    let snaplen = read_u32(bytes, 16, endian).ok_or(PcapError::TruncatedGlobalHeader)?;
    let network = read_u32(bytes, 20, endian).ok_or(PcapError::TruncatedGlobalHeader)?;
    let link_type = network & 0x0fff_ffff;
    if link_type != LINKTYPE_ETHERNET {
        return Err(PcapError::UnsupportedLinkType(link_type));
    }

    let mut packets = Vec::new();
    let mut offset = GLOBAL_HEADER_LEN;
    while offset < bytes.len() {
        let remaining = bytes.len().saturating_sub(offset);
        if remaining < RECORD_HEADER_LEN {
            return Err(PcapError::TruncatedFrame {
                offset,
                need: offset.saturating_add(RECORD_HEADER_LEN),
                got: bytes.len(),
            });
        }
        let ts_sec = read_u32(bytes, offset, endian).unwrap_or_default();
        let ts_frac = read_u32(bytes, offset + 4, endian).unwrap_or_default();
        let caplen = read_u32(bytes, offset + 8, endian).unwrap_or_default();
        let orig_len = read_u32(bytes, offset + 12, endian).unwrap_or_default();
        if caplen > snaplen || caplen > orig_len {
            return Err(PcapError::BadRecordLengths {
                caplen,
                orig_len,
                snaplen,
            });
        }
        let data_start = offset.saturating_add(RECORD_HEADER_LEN);
        let caplen_usize = usize::try_from(caplen).map_err(|_| PcapError::BadRecordLengths {
            caplen,
            orig_len,
            snaplen,
        })?;
        let data_end = data_start
            .checked_add(caplen_usize)
            .ok_or(PcapError::BadRecordLengths {
                caplen,
                orig_len,
                snaplen,
            })?;
        let data = bytes.get(data_start..data_end).ok_or({
            PcapError::TruncatedFrame {
                offset,
                need: data_start.saturating_add(caplen_usize),
                got: bytes.len(),
            }
        })?;
        packets.push(Packet {
            ts_sec,
            ts_frac,
            precision,
            caplen,
            orig_len,
            data,
        });
        offset = data_end;
    }

    Ok(PcapFile {
        link_type,
        snaplen,
        precision,
        packets,
    })
}

/// Serialise packets as a little-endian classic-pcap image.
#[must_use]
pub fn write(packets: &[Packet<'_>], link_type: u32, snaplen: u32) -> Vec<u8> {
    let precision = packets
        .first()
        .map_or(TimestampPrecision::Microseconds, |packet| packet.precision);
    let magic = match precision {
        TimestampPrecision::Microseconds => MAGIC_MICRO,
        TimestampPrecision::Nanoseconds => MAGIC_NANO,
    };
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&magic.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&0i32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&snaplen.to_le_bytes());
    bytes.extend_from_slice(&link_type.to_le_bytes());
    for packet in packets {
        bytes.extend_from_slice(&packet.ts_sec.to_le_bytes());
        bytes.extend_from_slice(&packet.ts_frac.to_le_bytes());
        bytes.extend_from_slice(&packet.caplen.to_le_bytes());
        bytes.extend_from_slice(&packet.orig_len.to_le_bytes());
        bytes.extend_from_slice(packet.data);
    }
    bytes
}

#[cfg(test)]
#[path = "pcap_ut.rs"]
mod tests;
