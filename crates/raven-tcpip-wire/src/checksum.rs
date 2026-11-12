// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Internet checksum (RFC 1071), transport pseudo-headers and incremental checksum updates.
//!
//! The checksum is computed over a scatter of byte slices as if they were concatenated. An odd
//! final octet is padded on the right with zero, including when the odd octet ends one slice and
//! the next slice continues the word.

use crate::ipv4::Ipv4Addr;
use crate::ipv6::Ipv6Addr;

/// Fold a 32-bit accumulator down to 16 bits.
fn fold(mut sum: u32) -> u16 {
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    // `sum` is at most 0xffff after the loop.
    u16::try_from(sum).unwrap_or(0xffff)
}

/// Add `value` to a 64-bit one's-complement accumulator (end-around carry).
fn add_carry(sum: u64, value: u64) -> u64 {
    let (total, carry) = sum.overflowing_add(value);
    total + u64::from(carry)
}

/// Accumulate an even-length slice as native-endian 16-bit words, eight bytes at a time.
///
/// One's-complement addition does not depend on byte order (RFC 1071 section 2(B)), so the
/// words are loaded as the CPU stores them and the folded result is swapped once at the end.
fn accumulate(bytes: &[u8], mut sum: u64) -> u64 {
    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        if let Ok(word) = <[u8; 8]>::try_from(chunk) {
            sum = add_carry(sum, u64::from_ne_bytes(word));
        }
    }
    let tail = chunks.remainder();
    if !tail.is_empty() {
        let mut padded = [0u8; 8];
        padded[..tail.len()].copy_from_slice(tail);
        sum = add_carry(sum, u64::from_ne_bytes(padded));
    }
    sum
}

/// Fold a 64-bit one's-complement accumulator down to 16 bits.
fn fold_wide(mut sum: u64) -> u16 {
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    u16::try_from(sum).unwrap_or(0xffff)
}

/// Compute the internet checksum of the concatenation of `parts`.
///
/// The result is the value a sender writes into the checksum field. To verify a received
/// packet, pass every slice including the checksum field; a valid packet yields zero.
#[must_use]
pub fn internet_checksum(parts: &[&[u8]]) -> u16 {
    let mut sum: u64 = 0;
    let mut pending: Option<u8> = None;

    for part in parts {
        let mut rest: &[u8] = part;
        if let Some(high) = pending.take() {
            match rest.split_first() {
                Some((&low, tail)) => {
                    sum = add_carry(sum, u64::from(u16::from_ne_bytes([high, low])));
                    rest = tail;
                }
                None => {
                    pending = Some(high);
                    continue;
                }
            }
        }
        let (body, tail) = rest.split_at(rest.len() & !1);
        sum = accumulate(body, sum);
        pending = tail.first().copied();
    }

    if let Some(high) = pending {
        sum = add_carry(sum, u64::from(u16::from_ne_bytes([high, 0])));
    }
    let network_order = u16::from_be_bytes(fold_wide(sum).to_ne_bytes());
    !network_order
}

/// Incrementally update a checksum after one 16-bit word changed (RFC 1624).
///
/// `checksum` is the previous checksum, `old_word` the previous value of the changed word and
/// `new_word` its new value. The result is the checksum of the whole data after the change.
#[must_use]
pub fn incremental_update(checksum: u16, old_word: u16, new_word: u16) -> u16 {
    let sum = u32::from(!checksum) + u32::from(!old_word) + u32::from(new_word);
    !fold(sum)
}

/// Build the IPv4 pseudo-header for TCP, UDP or ICMPv6-style checksums (RFC 791).
#[must_use]
pub fn pseudo_header_v4(src: Ipv4Addr, dst: Ipv4Addr, protocol: u8, length: u16) -> [u8; 12] {
    let mut header = [0u8; 12];
    let src_octets = src.octets();
    let dst_octets = dst.octets();
    let [l0, l1] = length.to_be_bytes();
    header[..4].copy_from_slice(&src_octets);
    header[4..8].copy_from_slice(&dst_octets);
    header[9] = protocol;
    header[10] = l0;
    header[11] = l1;
    header
}

/// Build the IPv6 pseudo-header for TCP, UDP or ICMPv6 checksums (RFC 8200 section 8.1).
#[must_use]
pub fn pseudo_header_v6(src: Ipv6Addr, dst: Ipv6Addr, next_header: u8, length: u32) -> [u8; 40] {
    let mut header = [0u8; 40];
    header[..16].copy_from_slice(&src.octets());
    header[16..32].copy_from_slice(&dst.octets());
    header[32..36].copy_from_slice(&length.to_be_bytes());
    header[39] = next_header;
    header
}

/// The address family and endpoints used to validate a transport checksum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumContext {
    /// IPv4 endpoints.
    Ipv4 {
        /// Source address from the IP header.
        src: Ipv4Addr,
        /// Destination address from the IP header.
        dst: Ipv4Addr,
    },
    /// IPv6 endpoints.
    Ipv6 {
        /// Source address from the IP header.
        src: Ipv6Addr,
        /// Destination address from the IP header.
        dst: Ipv6Addr,
    },
}

impl ChecksumContext {
    /// A checksum context for IPv4.
    #[must_use]
    pub const fn ipv4(src: Ipv4Addr, dst: Ipv4Addr) -> Self {
        Self::Ipv4 { src, dst }
    }

    /// A checksum context for IPv6.
    #[must_use]
    pub const fn ipv6(src: Ipv6Addr, dst: Ipv6Addr) -> Self {
        Self::Ipv6 { src, dst }
    }

    /// Compute the checksum of `segment`, which must already have its checksum field zeroed.
    #[must_use]
    pub fn compute(&self, protocol: u8, segment: &[u8]) -> u16 {
        let length = u32::try_from(segment.len()).unwrap_or(u32::MAX);
        match self {
            Self::Ipv4 { src, dst } => {
                let short = u16::try_from(segment.len()).unwrap_or(u16::MAX);
                let header = pseudo_header_v4(*src, *dst, protocol, short);
                internet_checksum(&[&header, segment])
            }
            Self::Ipv6 { src, dst } => {
                let header = pseudo_header_v6(*src, *dst, protocol, length);
                internet_checksum(&[&header, segment])
            }
        }
    }

    /// Verify a received segment whose checksum field is still in place.
    #[must_use]
    pub fn verify(&self, protocol: u8, segment: &[u8]) -> bool {
        let length = u32::try_from(segment.len()).unwrap_or(u32::MAX);
        match self {
            Self::Ipv4 { src, dst } => {
                let short = u16::try_from(segment.len()).unwrap_or(u16::MAX);
                let header = pseudo_header_v4(*src, *dst, protocol, short);
                internet_checksum(&[&header, segment]) == 0
            }
            Self::Ipv6 { src, dst } => {
                let header = pseudo_header_v6(*src, *dst, protocol, length);
                internet_checksum(&[&header, segment]) == 0
            }
        }
    }

    /// Whether this context is IPv6.
    #[must_use]
    pub const fn is_ipv6(&self) -> bool {
        matches!(self, Self::Ipv6 { .. })
    }
}
