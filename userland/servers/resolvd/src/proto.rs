// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `resolv:` control protocol.
//!
//! A client opens `resolv:` and sends one `CALL` with a query; the reply carries
//! either A/AAAA records, NXDOMAIN, or a bounded error.  The encoding is a
//! fixed little-endian layout, not a memory cast.

use alloc::vec::Vec;

/// Protocol version.
pub const VERSION: u16 = 1;
/// Query type A.
pub const QTYPE_A: u8 = 1;
/// Query type AAAA.
pub const QTYPE_AAAA: u8 = 28;
/// Maximum query name length.
pub const MAX_NAME: usize = 253;
/// Maximum records in one reply.
pub const MAX_RECORDS: usize = 8;
/// Reply status: answers follow.
pub const STATUS_OK: u8 = 0;
/// Reply status: the name does not exist.
pub const STATUS_NXDOMAIN: u8 = 1;
/// Reply status: the resolver failed before a DNS answer.
pub const STATUS_ERROR: u8 = 2;

/// A protocol error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtoError {
    /// Input ended before the fixed fields or payload.
    TooShort,
    /// Input continued after the message.
    TrailingBytes,
    /// Version is not [`VERSION`].
    BadVersion,
    /// Query type is not A or AAAA.
    BadType,
    /// Name is empty, too long, or has an invalid label.
    BadName,
    /// Record count or address width is invalid.
    BadLength,
    /// Status value is unknown.
    BadStatus,
}

/// One decoded query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Query<'a> {
    /// A or AAAA.
    pub qtype: u8,
    /// ASCII query name.
    pub name: &'a str,
}

impl<'a> Query<'a> {
    /// Encode a query into `out`; returns the number of bytes written.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        validate_name(self.name)?;
        if !matches!(self.qtype, QTYPE_A | QTYPE_AAAA) {
            return Err(ProtoError::BadType);
        }
        let total = 5 + self.name.len();
        if out.len() < total {
            return Err(ProtoError::TooShort);
        }
        out[..2].copy_from_slice(&VERSION.to_le_bytes());
        out[2] = self.qtype;
        out[3..5].copy_from_slice(&(self.name.len() as u16).to_le_bytes());
        out[5..total].copy_from_slice(self.name.as_bytes());
        Ok(total)
    }

    /// Decode one complete query.
    pub fn decode(input: &'a [u8]) -> Result<Self, ProtoError> {
        if input.len() < 5 {
            return Err(ProtoError::TooShort);
        }
        if u16::from_le_bytes([input[0], input[1]]) != VERSION {
            return Err(ProtoError::BadVersion);
        }
        let qtype = input[2];
        if !matches!(qtype, QTYPE_A | QTYPE_AAAA) {
            return Err(ProtoError::BadType);
        }
        let length = usize::from(u16::from_le_bytes([input[3], input[4]]));
        if length > MAX_NAME {
            return Err(ProtoError::BadLength);
        }
        let total = 5 + length;
        if input.len() < total {
            return Err(ProtoError::TooShort);
        }
        if input.len() > total {
            return Err(ProtoError::TrailingBytes);
        }
        let name = core::str::from_utf8(&input[5..total]).map_err(|_| ProtoError::BadName)?;
        validate_name(name)?;
        Ok(Self { qtype, name })
    }
}

/// One answer address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Address {
    /// An A record.
    V4([u8; 4]),
    /// An AAAA record.
    V6([u8; 16]),
}

/// One answer record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    /// TTL in seconds.
    pub ttl: u32,
    /// The address.
    pub address: Address,
}

/// A decoded reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// Status byte.
    pub status: u8,
    /// Query type the records answer.
    pub qtype: u8,
    /// Answer records in server order.
    pub records: Vec<Record>,
}

impl Reply {
    /// Build a successful reply.
    pub fn answers(qtype: u8, records: Vec<Record>) -> Self {
        Self {
            status: STATUS_OK,
            qtype,
            records,
        }
    }

    /// Build an NXDOMAIN reply.
    pub fn nxdomain(qtype: u8) -> Self {
        Self {
            status: STATUS_NXDOMAIN,
            qtype,
            records: Vec::new(),
        }
    }

    /// Build an error reply.
    pub fn error(qtype: u8) -> Self {
        Self {
            status: STATUS_ERROR,
            qtype,
            records: Vec::new(),
        }
    }

    /// Encode a reply into `out`; returns the number of bytes written.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if !matches!(self.qtype, QTYPE_A | QTYPE_AAAA) {
            return Err(ProtoError::BadType);
        }
        if !matches!(self.status, STATUS_OK | STATUS_NXDOMAIN | STATUS_ERROR) {
            return Err(ProtoError::BadStatus);
        }
        if self.records.len() > MAX_RECORDS {
            return Err(ProtoError::BadLength);
        }
        let record_len = if self.qtype == QTYPE_A {
            1 + 4 + 4
        } else {
            1 + 4 + 16
        };
        let total = 5 + self.records.len() * record_len;
        if out.len() < total {
            return Err(ProtoError::TooShort);
        }
        out[..2].copy_from_slice(&VERSION.to_le_bytes());
        out[2] = self.status;
        out[3] = self.qtype;
        out[4] = self.records.len() as u8;
        let mut offset = 5;
        for record in &self.records {
            out[offset] = self.qtype;
            out[offset + 1..offset + 5].copy_from_slice(&record.ttl.to_le_bytes());
            match (self.qtype, record.address) {
                (QTYPE_A, Address::V4(address)) => {
                    out[offset + 5..offset + 9].copy_from_slice(&address);
                    offset += 9;
                }
                (QTYPE_AAAA, Address::V6(address)) => {
                    out[offset + 5..offset + 21].copy_from_slice(&address);
                    offset += 21;
                }
                _ => return Err(ProtoError::BadType),
            }
        }
        Ok(total)
    }

    /// Decode one complete reply.
    pub fn decode(input: &[u8]) -> Result<Self, ProtoError> {
        if input.len() < 5 {
            return Err(ProtoError::TooShort);
        }
        if u16::from_le_bytes([input[0], input[1]]) != VERSION {
            return Err(ProtoError::BadVersion);
        }
        let status = input[2];
        if !matches!(status, STATUS_OK | STATUS_NXDOMAIN | STATUS_ERROR) {
            return Err(ProtoError::BadStatus);
        }
        let qtype = input[3];
        if !matches!(qtype, QTYPE_A | QTYPE_AAAA) {
            return Err(ProtoError::BadType);
        }
        let count = usize::from(input[4]);
        if count > MAX_RECORDS {
            return Err(ProtoError::BadLength);
        }
        let record_len = if qtype == QTYPE_A { 9 } else { 21 };
        let total = 5 + count * record_len;
        if input.len() < total {
            return Err(ProtoError::TooShort);
        }
        if input.len() > total {
            return Err(ProtoError::TrailingBytes);
        }
        let mut records = Vec::with_capacity(count);
        let mut offset = 5;
        for _ in 0..count {
            if input[offset] != qtype {
                return Err(ProtoError::BadType);
            }
            let ttl = u32::from_le_bytes(
                input[offset + 1..offset + 5]
                    .try_into()
                    .map_err(|_| ProtoError::TooShort)?,
            );
            let address = if qtype == QTYPE_A {
                Address::V4(
                    input[offset + 5..offset + 9]
                        .try_into()
                        .map_err(|_| ProtoError::TooShort)?,
                )
            } else {
                Address::V6(
                    input[offset + 5..offset + 21]
                        .try_into()
                        .map_err(|_| ProtoError::TooShort)?,
                )
            };
            records.push(Record { ttl, address });
            offset += record_len;
        }
        Ok(Self {
            status,
            qtype,
            records,
        })
    }
}

/// Validate a DNS name as ASCII labels, with an optional trailing dot.
pub fn validate_name(name: &str) -> Result<(), ProtoError> {
    if name.is_empty() || name.len() > MAX_NAME || name.contains('\0') {
        return Err(ProtoError::BadName);
    }
    let trimmed = name.strip_suffix('.').unwrap_or(name);
    if trimmed.is_empty() {
        return Err(ProtoError::BadName);
    }
    for label in trimmed.split('.') {
        if label.is_empty()
            || label.len() > 63
            || !label.is_ascii()
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(ProtoError::BadName);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "proto_ut.rs"]
mod tests;
