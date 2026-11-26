// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Data Sequence Signal (DSS): data ACK, data sequence mapping, DATA_FIN and checksum.

use alloc::vec::Vec;

use crate::Error;

/// DSS flag: Data ACK present.
pub const FLAG_DATA_ACK: u8 = 0x01;
/// DSS flag: Data ACK is 64 bits.
pub const FLAG_ACK_64: u8 = 0x02;
/// DSS flag: data sequence mapping present.
pub const FLAG_HAS_MAP: u8 = 0x04;
/// DSS flag: data sequence number is 64 bits.
pub const FLAG_DSN_64: u8 = 0x08;
/// DSS flag: DATA_FIN.
pub const FLAG_DATA_FIN: u8 = 0x10;

/// A data sequence mapping carried by DSS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    /// Absolute data sequence number.
    pub data_seq: u64,
    /// Relative subflow sequence number.
    pub subflow_seq: u32,
    /// Mapped data length; 0 means an infinite mapping.
    pub data_len: u16,
    /// Mapping checksum when checksums were negotiated.
    pub checksum: Option<u16>,
    /// Encode `data_seq` as 64 bits (otherwise the low 32 bits).
    pub dsn_64: bool,
}

/// A decoded DSS option body (the four-byte MPTCP header is handled by [`crate::option`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataSequenceSignal {
    /// Connection-level Data ACK, if present.
    pub data_ack: Option<u64>,
    /// The Data ACK is encoded as 64 bits.
    pub ack_64: bool,
    /// Optional data sequence mapping.
    pub mapping: Option<Mapping>,
    /// The mapping carries DATA_FIN.
    pub data_fin: bool,
}

impl DataSequenceSignal {
    /// The DSS flags byte for this signal.
    #[must_use]
    pub fn flags(&self) -> u8 {
        let mut flags = 0;
        if self.data_ack.is_some() {
            flags |= FLAG_DATA_ACK;
            if self.ack_64 {
                flags |= FLAG_ACK_64;
            }
        }
        if let Some(mapping) = &self.mapping {
            flags |= FLAG_HAS_MAP;
            if mapping.dsn_64 {
                flags |= FLAG_DSN_64;
            }
            if self.data_fin {
                flags |= FLAG_DATA_FIN;
            }
        }
        flags
    }

    /// Encode the subtype body (everything after the four-byte option header).
    #[must_use]
    pub fn encode_body(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(24);
        if let Some(ack) = self.data_ack {
            if self.ack_64 {
                out.extend_from_slice(&ack.to_be_bytes());
            } else {
                out.extend_from_slice(&(ack as u32).to_be_bytes());
            }
        }
        if let Some(mapping) = &self.mapping {
            if mapping.dsn_64 {
                out.extend_from_slice(&mapping.data_seq.to_be_bytes());
            } else {
                out.extend_from_slice(&(mapping.data_seq as u32).to_be_bytes());
            }
            out.extend_from_slice(&mapping.subflow_seq.to_be_bytes());
            out.extend_from_slice(&mapping.data_len.to_be_bytes());
            if let Some(sum) = mapping.checksum {
                out.extend_from_slice(&sum.to_be_bytes());
            }
        }
        out
    }

    /// Decode a DSS body.
    ///
    /// `body` starts after the four-byte MPTCP header; `option_length` is the TCP option length
    /// including the header. The length must exactly match the flags and checksum presence.
    pub fn parse_body(body: &[u8], flags: u8, option_length: usize) -> Result<Self, Error> {
        if flags & !0x1f != 0 {
            return Err(Error::InvalidDssFlags);
        }
        let has_ack = flags & FLAG_DATA_ACK != 0;
        let ack_64 = flags & FLAG_ACK_64 != 0;
        let has_map = flags & FLAG_HAS_MAP != 0;
        let dsn_64 = flags & FLAG_DSN_64 != 0;
        let data_fin = flags & FLAG_DATA_FIN != 0;
        if (!has_ack && ack_64) || (!has_map && (dsn_64 || data_fin)) {
            return Err(Error::InvalidDssFlags);
        }

        let mut expected = 4;
        if has_ack {
            expected += if ack_64 { 8 } else { 4 };
        }
        if has_map {
            expected += if dsn_64 { 8 } else { 4 };
            expected += 4 + 2;
        }
        let with_checksum = option_length == expected + 2;
        if option_length != expected && !with_checksum {
            return Err(Error::BadLength);
        }

        let mut offset = 0;
        let data_ack = if has_ack {
            let value = if ack_64 {
                let bytes = body.get(offset..offset + 8).ok_or(Error::Truncated)?;
                let mut array = [0u8; 8];
                array.copy_from_slice(bytes);
                offset += 8;
                u64::from_be_bytes(array)
            } else {
                let bytes = body.get(offset..offset + 4).ok_or(Error::Truncated)?;
                let mut array = [0u8; 4];
                array.copy_from_slice(bytes);
                offset += 4;
                u64::from(u32::from_be_bytes(array))
            };
            Some(value)
        } else {
            None
        };

        let mapping = if has_map {
            let data_seq = if dsn_64 {
                let bytes = body.get(offset..offset + 8).ok_or(Error::Truncated)?;
                let mut array = [0u8; 8];
                array.copy_from_slice(bytes);
                offset += 8;
                u64::from_be_bytes(array)
            } else {
                let bytes = body.get(offset..offset + 4).ok_or(Error::Truncated)?;
                let mut array = [0u8; 4];
                array.copy_from_slice(bytes);
                offset += 4;
                u64::from(u32::from_be_bytes(array))
            };
            let subflow_seq = {
                let bytes = body.get(offset..offset + 4).ok_or(Error::Truncated)?;
                let mut array = [0u8; 4];
                array.copy_from_slice(bytes);
                offset += 4;
                u32::from_be_bytes(array)
            };
            let data_len = {
                let bytes = body.get(offset..offset + 2).ok_or(Error::Truncated)?;
                offset += 2;
                u16::from_be_bytes([bytes[0], bytes[1]])
            };
            let checksum = if with_checksum {
                let bytes = body.get(offset..offset + 2).ok_or(Error::Truncated)?;
                Some(u16::from_be_bytes([bytes[0], bytes[1]]))
            } else {
                None
            };
            if data_len == 0 && checksum.is_some_and(|sum| sum != 0) {
                return Err(Error::InvalidDssFlags);
            }
            Some(Mapping {
                data_seq,
                subflow_seq,
                data_len,
                checksum,
                dsn_64,
            })
        } else {
            None
        };

        Ok(Self {
            data_ack,
            ack_64,
            mapping,
            data_fin,
        })
    }
}

/// RFC 8684 section 3.3.1 mapping checksum over the pseudo-header and payload.
///
/// This is the normal Internet checksum; the pseudo-header always uses the full 64-bit DSN.
#[must_use]
pub fn checksum(data_seq: u64, subflow_seq: u32, data_len: u16, payload: &[u8]) -> u16 {
    let mut sum = 0u32;
    let pseudo = [
        data_seq.to_be_bytes()[0],
        data_seq.to_be_bytes()[1],
        data_seq.to_be_bytes()[2],
        data_seq.to_be_bytes()[3],
        data_seq.to_be_bytes()[4],
        data_seq.to_be_bytes()[5],
        data_seq.to_be_bytes()[6],
        data_seq.to_be_bytes()[7],
        subflow_seq.to_be_bytes()[0],
        subflow_seq.to_be_bytes()[1],
        subflow_seq.to_be_bytes()[2],
        subflow_seq.to_be_bytes()[3],
        data_len.to_be_bytes()[0],
        data_len.to_be_bytes()[1],
        0,
        0,
    ];
    for chunk in pseudo.chunks_exact(2) {
        sum = sum.wrapping_add(u32::from(u16::from_be_bytes([chunk[0], chunk[1]])));
        sum = (sum & 0xffff) + (sum >> 16);
    }
    let mut bytes = payload.chunks_exact(2);
    for chunk in &mut bytes {
        sum = sum.wrapping_add(u32::from(u16::from_be_bytes([chunk[0], chunk[1]])));
        sum = (sum & 0xffff) + (sum >> 16);
    }
    let rest = bytes.remainder();
    if let Some(&byte) = rest.first() {
        sum = sum.wrapping_add(u32::from(u16::from_be_bytes([byte, 0])));
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// Verify a mapping checksum.
#[must_use]
pub fn verify_checksum(
    data_seq: u64,
    subflow_seq: u32,
    data_len: u16,
    payload: &[u8],
    expected: u16,
) -> bool {
    checksum(data_seq, subflow_seq, data_len, payload) == expected
}

#[cfg(test)]
#[path = "dss_ut.rs"]
mod tests;
