// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Linux `cmsghdr` arithmetic and iteration (`CMSG_ALIGN` is 8 bytes).
use crate::ErrnoExt;

use raven_libc_abi::errno::Errno;

/// The fixed `cmsghdr` size.
pub const HEADER_LEN: usize = 16;

/// One control message header and its data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cmsg<'a> {
    /// The `cmsg_level` field.
    pub level: i32,
    /// The `cmsg_type` field.
    pub ty: i32,
    /// The payload.
    pub data: &'a [u8],
}

/// `CMSG_ALIGN(n)`: round up to the 8-byte boundary.
#[must_use]
pub const fn align(n: usize) -> usize {
    n.saturating_add(7) & !7
}

/// `CMSG_SPACE(n)`: header plus aligned payload.
#[must_use]
pub const fn space(n: usize) -> usize {
    HEADER_LEN.saturating_add(align(n))
}

/// `CMSG_LEN(n)`: header plus payload, no trailing padding.
#[must_use]
pub const fn len(n: usize) -> usize {
    HEADER_LEN.saturating_add(n)
}

/// Iterate over a control buffer.
///
/// A length below the header, or a header that does not fit, yields one `EINVAL` and stops.
pub fn iter_cmsgs(mut buffer: &[u8]) -> impl Iterator<Item = Result<Cmsg<'_>, Errno>> {
    core::iter::from_fn(move || {
        if buffer.is_empty() {
            return None;
        }
        let Some(length_bytes) = buffer.get(..HEADER_LEN) else {
            buffer = &[];
            return Some(Err(Errno::EINVAL));
        };
        let length = read_usize(length_bytes);
        if length < HEADER_LEN || length > buffer.len() {
            buffer = &[];
            return Some(Err(Errno::EINVAL));
        }
        let Some(message) = buffer.get(..length) else {
            buffer = &[];
            return Some(Err(Errno::EINVAL));
        };
        let level_bytes = message.get(8..12).unwrap_or(&[]);
        let type_bytes = message.get(12..16).unwrap_or(&[]);
        let level = i32::from_le_bytes([
            *level_bytes.first().unwrap_or(&0),
            *level_bytes.get(1).unwrap_or(&0),
            *level_bytes.get(2).unwrap_or(&0),
            *level_bytes.get(3).unwrap_or(&0),
        ]);
        let ty = i32::from_le_bytes([
            *type_bytes.first().unwrap_or(&0),
            *type_bytes.get(1).unwrap_or(&0),
            *type_bytes.get(2).unwrap_or(&0),
            *type_bytes.get(3).unwrap_or(&0),
        ]);
        let data = message.get(HEADER_LEN..).unwrap_or(&[]);
        let advance = align(length).max(1);
        buffer = buffer.get(advance..).unwrap_or(&[]);
        Some(Ok(Cmsg { level, ty, data }))
    })
}

/// Encode one control message; returns `CMSG_LEN(n)` or `None` when `out` is too small.
pub fn encode_cmsg(level: i32, ty: i32, data: &[u8], out: &mut [u8]) -> Option<usize> {
    let full = len(data.len());
    let total = space(data.len());
    if out.len() < total {
        return None;
    }
    let header = out.get_mut(..HEADER_LEN)?;
    header.fill(0);
    header
        .get_mut(..8)?
        .copy_from_slice(&(full as u64).to_le_bytes());
    header.get_mut(8..12)?.copy_from_slice(&level.to_le_bytes());
    header.get_mut(12..16)?.copy_from_slice(&ty.to_le_bytes());
    out.get_mut(HEADER_LEN..full)?.copy_from_slice(data);
    out.get_mut(full..total)?.fill(0);
    Some(full)
}

fn read_usize(bytes: &[u8]) -> usize {
    let mut value = 0usize;
    for index in 0..8usize {
        if let Some(byte) = bytes.get(index) {
            value |= usize::from(*byte) << (index * 8);
        }
    }
    value
}

#[cfg(test)]
#[path = "cmsg_ut.rs"]
mod tests;
