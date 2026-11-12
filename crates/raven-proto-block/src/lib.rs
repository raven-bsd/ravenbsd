// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Wire protocol for the `block:` server and the GPT partition scanner.
//!
//! Requests are little-endian fixed headers carried in an S11 `CALL` buffer; the
//! server never casts a byte slice to a struct.  Every length is checked before
//! use and every sector range is checked against the selected device.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;

pub mod gpt;

pub use gpt::{Gpt, GptError, Partition};

/// Logical sector size in bytes.
pub const SECTOR_SIZE: u32 = 512;
/// Maximum sectors in one read or write request.
pub const MAX_SECTORS: u32 = 128;
/// Encoded request header size.
pub const REQUEST_HEADER_LEN: usize = 24;
/// Encoded reply header size.
pub const REPLY_HEADER_LEN: usize = 8;
/// Encoded `BlockInfo` size.
pub const INFO_LEN: usize = 24;
/// Encoded partition record size.
pub const PARTITION_LEN: usize = 64;
/// Encoded `LIST` payload header carrying the record count.
pub const LIST_HEADER_LEN: usize = 4;
/// Maximum partition records in one `LIST` reply.
pub const MAX_PARTITIONS: usize = 128;
/// Protocol version.
pub const VERSION: u16 = 1;

/// One request operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Read sectors into the reply payload.
    Read = 1,
    /// Write sectors from the request payload.
    Write = 2,
    /// Flush the device.
    Flush = 3,
    /// Return [`BlockInfo`].
    Info = 4,
    /// Return [`PartitionRecord`]s.
    List = 5,
}

impl Op {
    /// Parse a wire operation.
    pub const fn from_u16(value: u16) -> Option<Self> {
        Some(match value {
            1 => Self::Read,
            2 => Self::Write,
            3 => Self::Flush,
            4 => Self::Info,
            5 => Self::List,
            _ => return None,
        })
    }
}

/// A protocol framing or validation error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtoError {
    /// Input ended before the fixed header.
    TooShort,
    /// Input continued after the request ended.
    TrailingBytes,
    /// Version is not [`VERSION`].
    BadVersion,
    /// Operation is unknown.
    UnknownOp,
    /// Count or data length is outside the documented bound.
    BadLength,
    /// Reserved fields are nonzero or the operation/count combination is invalid.
    BadField,
}

/// A decoded request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request<'a> {
    /// Operation.
    pub op: Op,
    /// First sector relative to the selected device.
    pub sector: u64,
    /// Sector count.
    pub count: u32,
    /// Write payload (empty for other operations).
    pub data: &'a [u8],
}

impl<'a> Request<'a> {
    /// Validate and decode one request.
    pub fn decode(input: &'a [u8]) -> Result<Self, ProtoError> {
        if input.len() < REQUEST_HEADER_LEN {
            return Err(ProtoError::TooShort);
        }
        if u16::from_le_bytes([input[0], input[1]]) != VERSION {
            return Err(ProtoError::BadVersion);
        }
        let op =
            Op::from_u16(u16::from_le_bytes([input[2], input[3]])).ok_or(ProtoError::UnknownOp)?;
        let sector = u64::from_le_bytes(input[4..12].try_into().map_err(|_| ProtoError::TooShort)?);
        let count = u32::from_le_bytes(input[12..16].try_into().map_err(|_| ProtoError::TooShort)?);
        let flags = u32::from_le_bytes(input[16..20].try_into().map_err(|_| ProtoError::TooShort)?);
        let data_len =
            u32::from_le_bytes(input[20..24].try_into().map_err(|_| ProtoError::TooShort)?);
        if flags != 0 {
            return Err(ProtoError::BadField);
        }
        let expected = match op {
            Op::Write => {
                if count == 0 || count > MAX_SECTORS {
                    return Err(ProtoError::BadLength);
                }
                u64::from(count)
                    .checked_mul(u64::from(SECTOR_SIZE))
                    .ok_or(ProtoError::BadLength)?
            }
            Op::Read => {
                if count == 0 || count > MAX_SECTORS {
                    return Err(ProtoError::BadLength);
                }
                0
            }
            Op::Flush | Op::Info | Op::List => {
                if count != 0 || sector != 0 {
                    return Err(ProtoError::BadField);
                }
                0
            }
        };
        if u64::from(data_len) != expected {
            return Err(ProtoError::BadLength);
        }
        let total = REQUEST_HEADER_LEN
            .checked_add(usize::try_from(data_len).map_err(|_| ProtoError::BadLength)?)
            .ok_or(ProtoError::BadLength)?;
        if input.len() != total {
            return Err(if input.len() < total {
                ProtoError::TooShort
            } else {
                ProtoError::TrailingBytes
            });
        }
        let data = input
            .get(REQUEST_HEADER_LEN..)
            .ok_or(ProtoError::TooShort)?;
        Ok(Self {
            op,
            sector,
            count,
            data,
        })
    }

    /// Encode this request into `out`; returns the number of bytes written.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let expected = match self.op {
            Op::Write => {
                if self.count == 0 || self.count > MAX_SECTORS {
                    return Err(ProtoError::BadLength);
                }
                u64::from(self.count)
                    .checked_mul(u64::from(SECTOR_SIZE))
                    .ok_or(ProtoError::BadLength)?
            }
            Op::Read => {
                if self.count == 0 || self.count > MAX_SECTORS {
                    return Err(ProtoError::BadLength);
                }
                0
            }
            Op::Flush | Op::Info | Op::List => {
                if self.count != 0 || self.sector != 0 {
                    return Err(ProtoError::BadField);
                }
                0
            }
        };
        if u64::try_from(self.data.len()).map_err(|_| ProtoError::BadLength)? != expected {
            return Err(ProtoError::BadLength);
        }
        let total = REQUEST_HEADER_LEN
            .checked_add(self.data.len())
            .ok_or(ProtoError::BadLength)?;
        if out.len() < total {
            return Err(ProtoError::TooShort);
        }
        out[..2].copy_from_slice(&VERSION.to_le_bytes());
        out[2..4].copy_from_slice(&(self.op as u16).to_le_bytes());
        out[4..12].copy_from_slice(&self.sector.to_le_bytes());
        out[12..16].copy_from_slice(&self.count.to_le_bytes());
        out[16..20].copy_from_slice(&0u32.to_le_bytes());
        out[20..24].copy_from_slice(&u32::try_from(self.data.len()).unwrap_or(0).to_le_bytes());
        out[REQUEST_HEADER_LEN..total].copy_from_slice(self.data);
        Ok(total)
    }
}

/// Fixed device information.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockInfo {
    /// Logical sector size.
    pub sector_size: u32,
    /// Nonzero for a read-only device.
    pub read_only: u32,
    /// Device capacity in sectors.
    pub sectors: u64,
    /// Reserved.
    pub reserved: u64,
}

impl BlockInfo {
    /// Encode exactly [`INFO_LEN`] bytes.
    pub fn encode(&self, out: &mut [u8]) -> Result<(), ProtoError> {
        if out.len() < INFO_LEN {
            return Err(ProtoError::TooShort);
        }
        out[..4].copy_from_slice(&self.sector_size.to_le_bytes());
        out[4..8].copy_from_slice(&self.read_only.to_le_bytes());
        out[8..16].copy_from_slice(&self.sectors.to_le_bytes());
        out[16..24].copy_from_slice(&self.reserved.to_le_bytes());
        Ok(())
    }

    /// Decode exactly [`INFO_LEN`] bytes.
    pub fn decode(input: &[u8]) -> Result<Self, ProtoError> {
        if input.len() != INFO_LEN {
            return Err(if input.len() < INFO_LEN {
                ProtoError::TooShort
            } else {
                ProtoError::TrailingBytes
            });
        }
        Ok(Self {
            sector_size: u32::from_le_bytes(
                input[0..4].try_into().map_err(|_| ProtoError::TooShort)?,
            ),
            read_only: u32::from_le_bytes(
                input[4..8].try_into().map_err(|_| ProtoError::TooShort)?,
            ),
            sectors: u64::from_le_bytes(input[8..16].try_into().map_err(|_| ProtoError::TooShort)?),
            reserved: u64::from_le_bytes(
                input[16..24].try_into().map_err(|_| ProtoError::TooShort)?,
            ),
        })
    }
}

/// One partition record in a `LIST` reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartitionRecord {
    /// One-based partition index.
    pub index: u32,
    /// First sector relative to the disk.
    pub start_lba: u64,
    /// Sector count.
    pub sectors: u64,
    /// UTF-8 name, zero padded.
    pub name: [u8; 40],
    /// Name length.
    pub name_len: u8,
}

impl Default for PartitionRecord {
    fn default() -> Self {
        Self {
            index: 0,
            start_lba: 0,
            sectors: 0,
            name: [0; 40],
            name_len: 0,
        }
    }
}

impl PartitionRecord {
    /// Encode exactly [`PARTITION_LEN`] bytes.
    pub fn encode(&self, out: &mut [u8]) -> Result<(), ProtoError> {
        if out.len() < PARTITION_LEN || self.name_len as usize > 40 {
            return Err(ProtoError::BadLength);
        }
        out[..4].copy_from_slice(&self.index.to_le_bytes());
        out[4..12].copy_from_slice(&self.start_lba.to_le_bytes());
        out[12..20].copy_from_slice(&self.sectors.to_le_bytes());
        out[20..60].copy_from_slice(&self.name);
        out[60] = self.name_len;
        out[61..64].fill(0);
        Ok(())
    }

    /// Decode exactly [`PARTITION_LEN`] bytes.
    pub fn decode(input: &[u8]) -> Result<Self, ProtoError> {
        if input.len() != PARTITION_LEN {
            return Err(if input.len() < PARTITION_LEN {
                ProtoError::TooShort
            } else {
                ProtoError::TrailingBytes
            });
        }
        let name_len = input[60];
        if name_len as usize > 40 {
            return Err(ProtoError::BadLength);
        }
        let mut name = [0u8; 40];
        name.copy_from_slice(&input[20..60]);
        Ok(Self {
            index: u32::from_le_bytes(input[0..4].try_into().map_err(|_| ProtoError::TooShort)?),
            start_lba: u64::from_le_bytes(
                input[4..12].try_into().map_err(|_| ProtoError::TooShort)?,
            ),
            sectors: u64::from_le_bytes(
                input[12..20].try_into().map_err(|_| ProtoError::TooShort)?,
            ),
            name,
            name_len,
        })
    }
}

/// Encode a reply header and optional payload; returns bytes written.
pub fn encode_reply(out: &mut [u8], status: i32, payload: &[u8]) -> Result<usize, ProtoError> {
    write_reply_header(out, status, payload.len())?;
    let total = REPLY_HEADER_LEN + payload.len();
    out[REPLY_HEADER_LEN..total].copy_from_slice(payload);
    Ok(total)
}

/// Encode only the reply header for `payload_len` bytes already stored at
/// offset [`REPLY_HEADER_LEN`].
///
/// This lets a server fill a large read payload in place instead of copying it
/// through a temporary buffer.
pub fn write_reply_header(
    out: &mut [u8],
    status: i32,
    payload_len: usize,
) -> Result<(), ProtoError> {
    if out.len() < REPLY_HEADER_LEN {
        return Err(ProtoError::TooShort);
    }
    let length = u32::try_from(payload_len).map_err(|_| ProtoError::BadLength)?;
    out[..4].copy_from_slice(&status.to_le_bytes());
    out[4..8].copy_from_slice(&length.to_le_bytes());
    Ok(())
}

/// Encode a `LIST` payload: a little-endian count followed by the records.
/// Returns the number of payload bytes written.
pub fn encode_partition_list(
    out: &mut [u8],
    records: &[PartitionRecord],
) -> Result<usize, ProtoError> {
    if records.len() > MAX_PARTITIONS {
        return Err(ProtoError::BadLength);
    }
    let total = LIST_HEADER_LEN
        .checked_add(
            records
                .len()
                .checked_mul(PARTITION_LEN)
                .ok_or(ProtoError::BadLength)?,
        )
        .ok_or(ProtoError::BadLength)?;
    if out.len() < total {
        return Err(ProtoError::TooShort);
    }
    out[..4].copy_from_slice(
        &u32::try_from(records.len())
            .map_err(|_| ProtoError::BadLength)?
            .to_le_bytes(),
    );
    for (index, record) in records.iter().enumerate() {
        let offset = LIST_HEADER_LEN + index * PARTITION_LEN;
        record.encode(&mut out[offset..offset + PARTITION_LEN])?;
    }
    Ok(total)
}

/// Decode a `LIST` payload, rejecting a count beyond [`MAX_PARTITIONS`] and
/// any trailing bytes.
pub fn decode_partition_list(input: &[u8]) -> Result<Vec<PartitionRecord>, ProtoError> {
    if input.len() < LIST_HEADER_LEN {
        return Err(ProtoError::TooShort);
    }
    let count = usize::try_from(u32::from_le_bytes(
        input[..4].try_into().map_err(|_| ProtoError::TooShort)?,
    ))
    .map_err(|_| ProtoError::BadLength)?;
    if count > MAX_PARTITIONS {
        return Err(ProtoError::BadLength);
    }
    let expected = LIST_HEADER_LEN
        .checked_add(
            count
                .checked_mul(PARTITION_LEN)
                .ok_or(ProtoError::BadLength)?,
        )
        .ok_or(ProtoError::BadLength)?;
    if input.len() != expected {
        return Err(if input.len() < expected {
            ProtoError::TooShort
        } else {
            ProtoError::TrailingBytes
        });
    }
    let mut records = Vec::with_capacity(count);
    for index in 0..count {
        let offset = LIST_HEADER_LEN + index * PARTITION_LEN;
        records.push(PartitionRecord::decode(
            &input[offset..offset + PARTITION_LEN],
        )?);
    }
    Ok(records)
}

/// Decode a reply into status and payload.
pub fn decode_reply(input: &[u8]) -> Result<(i32, &[u8]), ProtoError> {
    if input.len() < REPLY_HEADER_LEN {
        return Err(ProtoError::TooShort);
    }
    let status = i32::from_le_bytes(input[0..4].try_into().map_err(|_| ProtoError::TooShort)?);
    let length = usize::try_from(u32::from_le_bytes(
        input[4..8].try_into().map_err(|_| ProtoError::TooShort)?,
    ))
    .map_err(|_| ProtoError::BadLength)?;
    if input.len() != REPLY_HEADER_LEN + length {
        return Err(if input.len() < REPLY_HEADER_LEN + length {
            ProtoError::TooShort
        } else {
            ProtoError::TrailingBytes
        });
    }
    Ok((status, &input[REPLY_HEADER_LEN..]))
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
