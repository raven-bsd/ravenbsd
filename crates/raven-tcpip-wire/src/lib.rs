// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-tcpip-wire`: zero-copy packet header parsing for the RavenBSD TCP/IP stack.
//!
//! Every parser in this crate is **total**: it either returns a borrowed view of the fields or a
//! [`ParseError`], and it never panics, never reads out of bounds and never allocates. Parsers do
//! not keep state; a view borrows the caller's byte slice.
//!
//! The wire formats implemented here are Ethernet II, ARP, IPv4, IPv6 (hop-by-hop, routing,
//! destination, fragment and authentication extension headers), ICMPv4, ICMPv6, UDP and TCP
//! (including the option set needed by the stack). Checksums follow RFC 1071, with the TCP/UDP
//! pseudo-header for IPv4 (RFC 791) and IPv6 (RFC 8200).
//!
//! The rejection tags are part of the contract pinned by `spec/tcpip/vectors/headers.toml`; see
//! [`ParseError::tag`].
//!
//! ```text
//! ParseError::Truncated { tag: "tcp.truncated", need: 20, got: 3 }
//! ```

#![no_std]
#![forbid(unsafe_code)]
#![allow(missing_docs)] // Wire field names are the RFC names; module docs explain their meaning.

#[cfg(test)]
extern crate std;

pub mod arp;
pub mod checksum;
pub mod ethernet;
pub mod icmpv4;
pub mod icmpv6;
pub mod ipv4;
pub mod ipv6;
pub mod tcp;
pub mod udp;

pub use arp::{ArpBuilder, ArpPacket};
pub use checksum::{
    ChecksumContext, incremental_update, internet_checksum, pseudo_header_v4, pseudo_header_v6,
};
pub use ethernet::{EtherType, EthernetBuilder, EthernetFrame, MacAddr};
pub use icmpv4::{Icmpv4Builder, Icmpv4Packet};
pub use icmpv6::{Icmpv6Builder, Icmpv6Packet};
pub use ipv4::{Ipv4Addr, Ipv4Builder, Ipv4Flags, Ipv4Header, Ipv4Packet};
pub use ipv6::{
    ExtensionKind, Extensions, FragmentHeader, Ipv6Addr, Ipv6Builder, Ipv6Extension, Ipv6Header,
    Ipv6Packet, write_extension, write_fragment,
};
pub use tcp::{SackBlocks, TcpBuilder, TcpFlags, TcpHeader, TcpOption, TcpOptions, TcpPacket};
pub use udp::{UdpBuilder, UdpPacket};

/// A typed parse rejection.
///
/// `tag` is the stable reason tag from the header-vector contract, for example `ipv4.checksum`
/// or `tcp.option_truncated`. Callers may match on the structured variants for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// The input ended before a required field. `need` is the minimum total length and `got` is
    /// the length actually supplied.
    Truncated {
        tag: &'static str,
        need: usize,
        got: usize,
    },
    /// The version nibble is not the one the protocol requires.
    BadVersion { tag: &'static str, version: u8 },
    /// A header-length field is outside its valid range or beyond the input.
    BadHeaderLen { tag: &'static str, value: u8 },
    /// The internet checksum did not verify.
    BadChecksum { tag: &'static str },
    /// A variable-length option is malformed at `offset` within the option area.
    BadOption { tag: &'static str, offset: usize },
    /// Any other pinned malformation, identified by its exact wire-contract tag.
    Malformed { tag: &'static str },
}

impl ParseError {
    /// Return the stable wire-contract reason tag.
    #[must_use]
    pub const fn tag(&self) -> &'static str {
        match self {
            Self::Truncated { tag, .. }
            | Self::BadVersion { tag, .. }
            | Self::BadHeaderLen { tag, .. }
            | Self::BadChecksum { tag }
            | Self::BadOption { tag, .. }
            | Self::Malformed { tag } => tag,
        }
    }
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated { tag, need, got } => {
                write!(f, "{tag}: truncated, need {need} bytes, got {got}")
            }
            Self::BadVersion { tag, version } => {
                write!(f, "{tag}: bad version {version}")
            }
            Self::BadHeaderLen { tag, value } => {
                write!(f, "{tag}: bad header length {value}")
            }
            Self::BadChecksum { tag } => write!(f, "{tag}: bad checksum"),
            Self::BadOption { tag, offset } => {
                write!(f, "{tag}: bad option at offset {offset}")
            }
            Self::Malformed { tag } => write!(f, "{tag}: malformed packet"),
        }
    }
}

/// A read of a single octet at `offset`.
pub(crate) fn read_u8(bytes: &[u8], offset: usize, tag: &'static str) -> Result<u8, ParseError> {
    match bytes.get(offset) {
        Some(&value) => Ok(value),
        None => Err(ParseError::Truncated {
            tag,
            need: offset.saturating_add(1),
            got: bytes.len(),
        }),
    }
}

/// A big-endian read of two octets at `offset`.
pub(crate) fn read_u16(bytes: &[u8], offset: usize, tag: &'static str) -> Result<u16, ParseError> {
    let end = offset.checked_add(2).ok_or(ParseError::Truncated {
        tag,
        need: usize::MAX,
        got: bytes.len(),
    })?;
    match bytes.get(offset..end) {
        Some(&[hi, lo]) => Ok(u16::from_be_bytes([hi, lo])),
        _ => Err(ParseError::Truncated {
            tag,
            need: end,
            got: bytes.len(),
        }),
    }
}

/// A big-endian read of four octets at `offset`.
pub(crate) fn read_u32(bytes: &[u8], offset: usize, tag: &'static str) -> Result<u32, ParseError> {
    let end = offset.checked_add(4).ok_or(ParseError::Truncated {
        tag,
        need: usize::MAX,
        got: bytes.len(),
    })?;
    match bytes.get(offset..end) {
        Some(&[a, b, c, d]) => Ok(u32::from_be_bytes([a, b, c, d])),
        _ => Err(ParseError::Truncated {
            tag,
            need: end,
            got: bytes.len(),
        }),
    }
}

/// Write a big-endian `u16` at `offset` if the slice is long enough.
pub(crate) fn write_u16(bytes: &mut [u8], offset: usize, value: u16) -> bool {
    let end = match offset.checked_add(2) {
        Some(end) => end,
        None => return false,
    };
    match bytes.get_mut(offset..end) {
        Some(slot) => {
            let [hi, lo] = value.to_be_bytes();
            slot.copy_from_slice(&[hi, lo]);
            true
        }
        None => false,
    }
}

/// Write a big-endian `u32` at `offset` if the slice is long enough.
pub(crate) fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> bool {
    let end = match offset.checked_add(4) {
        Some(end) => end,
        None => return false,
    };
    match bytes.get_mut(offset..end) {
        Some(slot) => {
            slot.copy_from_slice(&value.to_be_bytes());
            true
        }
        None => false,
    }
}

/// Check that `range` is inside `bytes` and return the sub-slice.
pub(crate) fn slice<'a>(
    bytes: &'a [u8],
    start: usize,
    end: usize,
    tag: &'static str,
) -> Result<&'a [u8], ParseError> {
    if end > bytes.len() {
        return Err(ParseError::Truncated {
            tag,
            need: end,
            got: bytes.len(),
        });
    }
    match bytes.get(start..end) {
        Some(part) => Ok(part),
        None => Err(ParseError::Truncated {
            tag,
            need: end,
            got: bytes.len(),
        }),
    }
}
