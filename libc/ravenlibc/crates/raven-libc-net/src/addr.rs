// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Linux `sockaddr` parsing and writing (DR-13; no BSD `sa_len`).
use crate::ErrnoExt;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::socket::{AF_INET, AF_INET6, AF_UNIX};
use raven_proto_net::Endpoint;

/// A socket address in host form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SockAddr {
    /// An IPv4 address and port.
    In {
        /// IPv4 octets in network order.
        ip: [u8; 4],
        /// Port in host order.
        port: u16,
    },
    /// An IPv6 address, port and scope.
    In6 {
        /// 16 address octets.
        ip: [u8; 16],
        /// Port in host order.
        port: u16,
        /// Scope id.
        scope: u32,
        /// Flow information.
        flowinfo: u32,
    },
    /// A Unix-domain path; an empty path is an unnamed pair address.
    Un {
        /// The raw path bytes, at most 108.
        path: [u8; 108],
        /// The path length.
        len: usize,
    },
}

impl SockAddr {
    /// The encoded size in bytes.
    #[must_use]
    pub const fn encoded_len(&self) -> usize {
        match self {
            Self::In { .. } => 16,
            Self::In6 { .. } => 28,
            Self::Un { .. } => 110,
        }
    }

    /// The Linux address family.
    #[must_use]
    pub const fn family(&self) -> u16 {
        match self {
            Self::In { .. } => AF_INET as u16,
            Self::In6 { .. } => AF_INET6 as u16,
            Self::Un { .. } => AF_UNIX as u16,
        }
    }

    /// Convert an IPv4 address to the wire endpoint.
    #[must_use]
    pub fn to_endpoint(&self) -> Option<Endpoint> {
        match self {
            Self::In { ip, port } => Some(Endpoint {
                ip: *ip,
                port: *port,
            }),
            _ => None,
        }
    }

    /// Build an IPv4 address from a wire endpoint.
    #[must_use]
    pub const fn from_endpoint(endpoint: Endpoint) -> Self {
        Self::In {
            ip: endpoint.ip,
            port: endpoint.port,
        }
    }

    /// The port in host order, or 0 for Unix addresses.
    #[must_use]
    pub const fn port(&self) -> u16 {
        match self {
            Self::In { port, .. } | Self::In6 { port, .. } => *port,
            Self::Un { .. } => 0,
        }
    }

    /// The IPv4 octets.
    #[must_use]
    pub const fn ipv4(&self) -> Option<[u8; 4]> {
        match self {
            Self::In { ip, .. } => Some(*ip),
            _ => None,
        }
    }

    /// The Unix path, without the trailing NUL.
    #[must_use]
    pub fn unix_path(&self) -> Option<&[u8]> {
        match self {
            Self::Un { path, len } => path.get(..*len),
            _ => None,
        }
    }
}

/// Parse a Linux `sockaddr_in`, `sockaddr_in6` or `sockaddr_un`.
pub fn parse_sockaddr(bytes: &[u8]) -> Result<SockAddr, Errno> {
    let family = read_u16(bytes, 0).ok_or(Errno::EINVAL)?;
    match i32::from(family) {
        AF_INET => {
            if bytes.len() < 16 {
                return Err(Errno::EINVAL);
            }
            let port = read_be_u16(bytes, 2).ok_or(Errno::EINVAL)?;
            let ip = read_four(bytes, 4).ok_or(Errno::EINVAL)?;
            Ok(SockAddr::In { ip, port })
        }
        AF_INET6 => {
            if bytes.len() < 28 {
                return Err(Errno::EINVAL);
            }
            let port = read_be_u16(bytes, 2).ok_or(Errno::EINVAL)?;
            let flowinfo = read_u32(bytes, 4).ok_or(Errno::EINVAL)?;
            let mut ip = [0u8; 16];
            if let Some(source) = bytes.get(8..24) {
                ip.copy_from_slice(source);
            } else {
                return Err(Errno::EINVAL);
            }
            let scope = read_u32(bytes, 24).ok_or(Errno::EINVAL)?;
            Ok(SockAddr::In6 {
                ip,
                port,
                scope,
                flowinfo,
            })
        }
        AF_UNIX => parse_unix(bytes),
        _ => Err(Errno::EAFNOSUPPORT),
    }
}

fn parse_unix(bytes: &[u8]) -> Result<SockAddr, Errno> {
    if bytes.len() < 2 {
        return Err(Errno::EINVAL);
    }
    let available = bytes.len().saturating_sub(2).min(108);
    let mut path = [0u8; 108];
    let mut len = 0usize;
    for (index, byte) in bytes.iter().skip(2).take(available).enumerate() {
        if *byte == 0 {
            break;
        }
        if let Some(slot) = path.get_mut(index) {
            *slot = *byte;
            len = index.saturating_add(1);
        }
    }
    Ok(SockAddr::Un { path, len })
}

/// Write an address into `out`; returns the full encoded length.
///
/// The caller sees POSIX truncation: only `min(full, out.len())` bytes are written, but the
/// full length is always returned.
pub fn write_sockaddr(addr: &SockAddr, out: &mut [u8]) -> usize {
    let full = addr.encoded_len();
    match addr {
        SockAddr::In { ip, port } => {
            put_u16(out, 0, AF_INET as u16);
            put_be_u16(out, 2, *port);
            put_bytes(out, 4, ip);
            if let Some(rest) = out.get_mut(8..16) {
                rest.fill(0);
            }
        }
        SockAddr::In6 {
            ip,
            port,
            scope,
            flowinfo,
        } => {
            put_u16(out, 0, AF_INET6 as u16);
            put_be_u16(out, 2, *port);
            put_u32(out, 4, *flowinfo);
            put_bytes(out, 8, ip);
            put_u32(out, 24, *scope);
        }
        SockAddr::Un { path, len } => {
            put_u16(out, 0, AF_UNIX as u16);
            put_bytes(out, 2, path.get(..*len).unwrap_or(&[]));
            if let Some(rest) = out.get_mut(2usize.saturating_add(*len)..110) {
                rest.fill(0);
            }
        }
    }
    full
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let pair = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*pair.first()?, *pair.get(1)?]))
}

fn read_be_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let pair = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes([*pair.first()?, *pair.get(1)?]))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let quad = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([
        *quad.first()?,
        *quad.get(1)?,
        *quad.get(2)?,
        *quad.get(3)?,
    ]))
}

fn read_four(bytes: &[u8], at: usize) -> Option<[u8; 4]> {
    let quad = bytes.get(at..at.checked_add(4)?)?;
    Some([*quad.first()?, *quad.get(1)?, *quad.get(2)?, *quad.get(3)?])
}

fn put_u16(out: &mut [u8], at: usize, value: u16) {
    if let Some(slot) = out.get_mut(at..at.saturating_add(2)) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn put_be_u16(out: &mut [u8], at: usize, value: u16) {
    if let Some(slot) = out.get_mut(at..at.saturating_add(2)) {
        slot.copy_from_slice(&value.to_be_bytes());
    }
}

fn put_u32(out: &mut [u8], at: usize, value: u32) {
    if let Some(slot) = out.get_mut(at..at.saturating_add(4)) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn put_bytes(out: &mut [u8], at: usize, bytes: &[u8]) {
    if let Some(slot) = out.get_mut(at..at.saturating_add(bytes.len())) {
        slot.copy_from_slice(bytes);
    }
}

#[cfg(test)]
#[path = "addr_ut.rs"]
mod tests;
