// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The fixed `resolv:` query/reply codec used by [`crate::SchemeResolver`].
//!
//! This mirrors the first-party `userland/servers/resolvd/src/proto.rs` layout: a five-byte
//! little-endian header, a bounded A record list and no compression. Keeping the codec here lets
//! the adapter be host-tested without a resolvd process; the server remains the policy owner.

use core::net::Ipv4Addr;

use crate::{ResolveError, Resolved};

/// Protocol version.
pub(crate) const VERSION: u16 = 1;
/// Query type A.
pub(crate) const QTYPE_A: u8 = 1;
/// Successful reply.
pub(crate) const STATUS_OK: u8 = 0;
/// Largest accepted DNS name.
pub(crate) const MAX_NAME: usize = 253;
/// Largest record list the server may return.
pub(crate) const MAX_RECORDS: usize = 8;

/// Encode one A query into `out`; returns the number of bytes written.
pub(crate) fn encode_query(name: &str, out: &mut [u8]) -> Result<usize, ResolveError> {
    if name.is_empty() || name.len() > MAX_NAME {
        return Err(ResolveError::NotFound);
    }
    for label in name.split('.') {
        if label.is_empty()
            || label.len() > 63
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(ResolveError::NotFound);
        }
    }
    let length = u16::try_from(name.len()).map_err(|_| ResolveError::NotFound)?;
    let total = 5 + name.len();
    let slot = out.get_mut(..total).ok_or(ResolveError::Io)?;
    slot[..2].copy_from_slice(&VERSION.to_le_bytes());
    slot[2] = QTYPE_A;
    slot[3..5].copy_from_slice(&length.to_le_bytes());
    slot[5..total].copy_from_slice(name.as_bytes());
    Ok(total)
}

/// Decode one A reply and return the first address and its TTL.
pub(crate) fn decode_reply(input: &[u8]) -> Result<Resolved, ResolveError> {
    if input.len() < 5 || u16::from_le_bytes([input[0], input[1]]) != VERSION || input[3] != QTYPE_A
    {
        return Err(ResolveError::Io);
    }
    if input[2] != STATUS_OK {
        return Err(ResolveError::NotFound);
    }
    let count = usize::from(input[4]);
    let total = 5 + count.saturating_mul(9);
    if count == 0 || count > MAX_RECORDS || input.len() != total {
        return Err(ResolveError::NotFound);
    }
    let record = input.get(5..14).ok_or(ResolveError::Io)?;
    if record[0] != QTYPE_A {
        return Err(ResolveError::Io);
    }
    let ttl_secs = u32::from_le_bytes([record[1], record[2], record[3], record[4]]);
    Ok(Resolved {
        address: Ipv4Addr::new(record[5], record[6], record[7], record[8]),
        ttl_secs,
    })
}

#[cfg(test)]
#[path = "resolv_ut.rs"]
mod tests;
