// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FreeBSD `sockaddr` parsing and writing (DR-69): `sa_len` at 0, one-byte family at 1.
//!
//! The public records are parsed and encoded exactly as FreeBSD `sys/sys/socket.h`,
//! `sys/netinet/in.h`, `sys/netinet6/in6.h` and `sys/sys/un.h` define them. Private
//! `raven_proto_net` endpoint records never see an address family or a length byte.
use crate::ErrnoExt;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::socket::{AF_INET, AF_INET6, AF_UNIX};
use raven_proto_net::Endpoint;

/// FreeBSD `SOCK_MAXADDRLEN`.
pub const SOCK_MAXADDRLEN: usize = 255;
/// `sizeof(struct sockaddr_in)`.
pub const SOCKADDR_IN_LEN: usize = 16;
/// `sizeof(struct sockaddr_in6)`.
pub const SOCKADDR_IN6_LEN: usize = 28;
/// `sizeof(struct sockaddr_un)`.
pub const SOCKADDR_UN_LEN: usize = 106;
/// FreeBSD `SUNPATHLEN`.
pub const SUN_PATH_LEN: usize = 104;
/// The unnamed Unix address is a plain `struct sockaddr` (`sun_noname`), 16 bytes.
pub const SUN_NONAME_LEN: usize = 16;

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
        /// The raw path bytes, at most `SUN_PATH_LEN`.
        path: [u8; SUN_PATH_LEN],
        /// The path length.
        len: usize,
    },
}

impl SockAddr {
    /// The encoded size in bytes (`SUN_LEN` for a Unix path).
    #[must_use]
    pub const fn encoded_len(&self) -> usize {
        match self {
            Self::In { .. } => SOCKADDR_IN_LEN,
            Self::In6 { .. } => SOCKADDR_IN6_LEN,
            Self::Un { len, .. } => {
                let len = if *len > SUN_PATH_LEN {
                    SUN_PATH_LEN
                } else {
                    *len
                };
                2usize.saturating_add(len)
            }
        }
    }

    /// The public address family.
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

/// Parse a FreeBSD `sockaddr_in`, `sockaddr_in6` or `sockaddr_un`.
///
/// `bytes` is the caller's `socklen`, which FreeBSD `getsockaddr` treats as authoritative and
/// copies into `sa_len`. The address type therefore has to match its exact record size; a
/// short or padded buffer is rejected instead of being parsed by an assumed width.
pub fn parse_sockaddr(bytes: &[u8]) -> Result<SockAddr, Errno> {
    if bytes.len() > SOCK_MAXADDRLEN {
        return Err(Errno::ENAMETOOLONG);
    }
    let family = *bytes.get(1).ok_or(Errno::EINVAL)?;
    match i32::from(family) {
        AF_INET => {
            if bytes.len() != SOCKADDR_IN_LEN {
                return Err(Errno::EINVAL);
            }
            let port = read_be_u16(bytes, 2).ok_or(Errno::EINVAL)?;
            let ip = read_four(bytes, 4).ok_or(Errno::EINVAL)?;
            Ok(SockAddr::In { ip, port })
        }
        AF_INET6 => {
            if bytes.len() != SOCKADDR_IN6_LEN {
                return Err(Errno::EINVAL);
            }
            let port = read_be_u16(bytes, 2).ok_or(Errno::EINVAL)?;
            let flowinfo = read_u32(bytes, 4).ok_or(Errno::EINVAL)?;
            let source = bytes.get(8..24).ok_or(Errno::EINVAL)?;
            let mut ip = [0u8; 16];
            ip.copy_from_slice(source);
            let scope = read_u32(bytes, 24).ok_or(Errno::EINVAL)?;
            Ok(SockAddr::In6 {
                ip,
                port,
                scope,
                flowinfo,
            })
        }
        AF_UNIX => {
            if bytes.len() > SOCKADDR_UN_LEN {
                return Err(Errno::EINVAL);
            }
            parse_unix(bytes)
        }
        _ => Err(Errno::EAFNOSUPPORT),
    }
}

fn parse_unix(bytes: &[u8]) -> Result<SockAddr, Errno> {
    let source = bytes.get(2..).ok_or(Errno::EINVAL)?;
    let mut path = [0u8; SUN_PATH_LEN];
    let mut len = 0usize;
    for (index, byte) in source.iter().take(SUN_PATH_LEN).enumerate() {
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

/// Write an address into `out`; returns the full encoded length (`sa_len`).
///
/// Only `min(full, out.len())` bytes are written; the full length is always returned, matching
/// FreeBSD `user_getsockname`'s `min(ss_len, len)` copy with the full `ss_len` in `*alen`.
pub fn write_sockaddr(addr: &SockAddr, out: &mut [u8]) -> usize {
    let full = addr.encoded_len();
    match addr {
        SockAddr::In { ip, port } => {
            put_u8(out, 0, full as u8);
            put_u8(out, 1, AF_INET as u8);
            put_be_u16(out, 2, *port);
            put_bytes(out, 4, ip);
            if let Some(rest) = out.get_mut(8..full) {
                rest.fill(0);
            }
        }
        SockAddr::In6 {
            ip,
            port,
            scope,
            flowinfo,
        } => {
            put_u8(out, 0, full as u8);
            put_u8(out, 1, AF_INET6 as u8);
            put_be_u16(out, 2, *port);
            put_u32(out, 4, *flowinfo);
            put_bytes(out, 8, ip);
            put_u32(out, 24, *scope);
        }
        SockAddr::Un { path, len } => {
            let path_len = (*len).min(SUN_PATH_LEN);
            put_u8(out, 0, full as u8);
            put_u8(out, 1, AF_UNIX as u8);
            put_bytes(out, 2, path.get(..path_len).unwrap_or(&[]));
        }
    }
    full
}

/// Write FreeBSD's `sun_noname`: a 16-byte `struct sockaddr` for an unbound Unix socket.
pub fn write_unnamed_unix(out: &mut [u8]) -> usize {
    let full = SUN_NONAME_LEN;
    put_u8(out, 0, full as u8);
    put_u8(out, 1, AF_UNIX as u8);
    if let Some(rest) = out.get_mut(2..full) {
        rest.fill(0);
    }
    full
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

fn put_u8(out: &mut [u8], at: usize, value: u8) {
    if let Some(slot) = out.get_mut(at) {
        *slot = value;
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
