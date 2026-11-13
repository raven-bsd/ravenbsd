// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The address, hint and result records shared by every module.

use alloc::string::String;

use crate::consts::{AF_INET, AF_INET6, AF_UNSPEC, SOCK_DGRAM, SOCK_STREAM};

/// An address family the resolver understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Family {
    /// IPv4 (`AF_INET`).
    V4,
    /// IPv6 (`AF_INET6`).
    V6,
}

impl Family {
    /// The Linux `AF_*` number.
    #[must_use]
    pub const fn af(self) -> i32 {
        match self {
            Self::V4 => AF_INET,
            Self::V6 => AF_INET6,
        }
    }

    /// Map a Linux `AF_*` number; `None` for anything except 2 and 10.
    #[must_use]
    pub const fn from_af(af: i32) -> Option<Self> {
        match af {
            AF_INET => Some(Self::V4),
            AF_INET6 => Some(Self::V6),
            _ => None,
        }
    }

    /// The other family.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::V4 => Self::V6,
            Self::V6 => Self::V4,
        }
    }

    /// The size of one address in bytes.
    #[must_use]
    pub const fn size(self) -> usize {
        match self {
            Self::V4 => 4,
            Self::V6 => 16,
        }
    }
}

/// A literal IP address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IpAddr {
    /// An IPv4 address.
    V4([u8; 4]),
    /// An IPv6 address.
    V6([u8; 16]),
}

impl IpAddr {
    /// The family.
    #[must_use]
    pub const fn family(self) -> Family {
        match self {
            Self::V4(_) => Family::V4,
            Self::V6(_) => Family::V6,
        }
    }

    /// The address bytes, 4 or 16 long.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::V4(a) => a,
            Self::V6(a) => a,
        }
    }

    /// The address as a 16-byte buffer; IPv4 becomes the v4-mapped `::ffff:a.b.c.d`.
    #[must_use]
    pub fn octets16(&self) -> [u8; 16] {
        let mut out = [0u8; 16];
        match self {
            Self::V4(a) => {
                if let Some(slot) = out.get_mut(10..12) {
                    slot.copy_from_slice(&[0xff, 0xff]);
                }
                if let Some(slot) = out.get_mut(12..) {
                    slot.copy_from_slice(a);
                }
            }
            Self::V6(a) => out = *a,
        }
        out
    }

    /// Build a 16-byte address into an `IpAddr` of the given family.
    #[must_use]
    pub fn from_octets16(family: Family, octets: &[u8; 16]) -> Self {
        match family {
            Family::V4 => {
                let mut a = [0u8; 4];
                if let Some(src) = octets.get(12..) {
                    a.copy_from_slice(src);
                }
                Self::V4(a)
            }
            Family::V6 => Self::V6(*octets),
        }
    }

    /// Whether this is the unspecified address (`0.0.0.0` or `::`).
    #[must_use]
    pub fn is_unspecified(&self) -> bool {
        self.as_bytes().iter().all(|byte| *byte == 0)
    }

    /// Whether this is a loopback address (`127.0.0.0/8` or `::1`).
    #[must_use]
    pub fn is_loopback(&self) -> bool {
        match self {
            Self::V4(a) => a.first().copied() == Some(127),
            Self::V6(a) => {
                let mut index = 0usize;
                while index < 15usize {
                    if a.get(index).copied() != Some(0) {
                        return false;
                    }
                    index = index.saturating_add(1);
                }
                a.get(15).copied() == Some(1)
            }
        }
    }

    /// The IPv6 link-local prefix (`fe80::/10`).
    #[must_use]
    pub fn is_link_local(&self) -> bool {
        match self {
            Self::V4(_) => false,
            Self::V6(a) => {
                a.first().copied() == Some(0xfe) && (a.get(1).copied().unwrap_or(0) & 0xc0) == 0x80
            }
        }
    }

    /// The IPv6 link-local multicast prefix (`ff02::/16`).
    #[must_use]
    pub fn is_mc_link_local(&self) -> bool {
        match self {
            Self::V4(_) => false,
            Self::V6(a) => a.first().copied() == Some(0xff) && a.get(1).copied() == Some(0x02),
        }
    }

    /// The IPv4-mapped IPv6 form, `::ffff:a.b.c.d`.
    #[must_use]
    pub fn to_v4_mapped(&self) -> Self {
        let mut out = [0u8; 16];
        match self {
            Self::V4(a) => {
                if let Some(slot) = out.get_mut(10..12) {
                    slot.copy_from_slice(&[0xff, 0xff]);
                }
                if let Some(slot) = out.get_mut(12..) {
                    slot.copy_from_slice(a);
                }
            }
            Self::V6(a) => out = *a,
        }
        Self::V6(out)
    }

    /// The IPv4 address inside a v4-mapped IPv6 address.
    #[must_use]
    pub fn as_v4_mapped(&self) -> Option<[u8; 4]> {
        match self {
            Self::V4(a) => Some(*a),
            Self::V6(a) => {
                if a.get(..10)
                    .is_some_and(|head| head.iter().all(|byte| *byte == 0))
                    && a.get(10).copied() == Some(0xff)
                    && a.get(11).copied() == Some(0xff)
                {
                    let mut out = [0u8; 4];
                    if let Some(src) = a.get(12..) {
                        out.copy_from_slice(src);
                    }
                    Some(out)
                } else {
                    None
                }
            }
        }
    }
}

/// A socket address without the platform `sockaddr` layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SockAddr {
    /// The literal address.
    pub ip: IpAddr,
    /// The port in host byte order.
    pub port: u16,
    /// The IPv6 scope id (`sin6_scope_id`).
    pub scope: u32,
}

impl SockAddr {
    /// A socket address with no scope.
    #[must_use]
    pub const fn new(ip: IpAddr, port: u16) -> Self {
        Self { ip, port, scope: 0 }
    }

    /// The same address with a scope id.
    #[must_use]
    pub const fn with_scope(ip: IpAddr, port: u16, scope: u32) -> Self {
        Self { ip, port, scope }
    }

    /// The family.
    #[must_use]
    pub const fn family(&self) -> Family {
        self.ip.family()
    }
}

/// The hints accepted by [`crate::getaddrinfo`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hints {
    /// `AI_*` flags.
    pub flags: i32,
    /// An `AF_*` family, or `AF_UNSPEC`.
    pub family: i32,
    /// `SOCK_*`, or 0.
    pub socktype: i32,
    /// An IP protocol number, or 0.
    pub protocol: i32,
}

impl Hints {
    /// The Linux `socktype` implied by the hints, if any.
    #[must_use]
    pub const fn socktype_name(self) -> Option<&'static str> {
        match self.socktype {
            SOCK_STREAM => Some("stream"),
            SOCK_DGRAM => Some("dgram"),
            _ => None,
        }
    }

    /// Whether the family field is one of the accepted values.
    #[must_use]
    pub const fn family_valid(self) -> bool {
        matches!(self.family, AF_UNSPEC | AF_INET | AF_INET6)
    }
}

/// One `addrinfo` record; the shell packs these into one block (T-12-17).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddrInfoRec {
    /// The `AI_*` flags copied from the hints.
    pub flags: i32,
    /// The Linux `AF_*` family.
    pub family: i32,
    /// `SOCK_STREAM` or `SOCK_DGRAM`.
    pub socktype: i32,
    /// The IP protocol number.
    pub protocol: i32,
    /// The socket address.
    pub addr: SockAddr,
    /// The canonical name, when the lookup produced one.
    pub canonname: Option<String>,
}

/// A UDP socket handle owned by the platform layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UdpSock(pub u32);

/// A TCP socket handle owned by the platform layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TcpSock(pub u32);

/// One received datagram.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Recv {
    /// The number of bytes written into the caller's buffer.
    pub len: usize,
    /// The source address.
    pub from: SockAddr,
    /// Whether the datagram was larger than the buffer (`MSG_TRUNC`).
    pub truncated: bool,
}

/// What a [`Interest`] refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterestKind {
    /// The shared UDP socket.
    Udp(UdpSock),
    /// A per-query TCP socket.
    Tcp(TcpSock),
}

/// One entry passed to [`crate::traits::DnsIo::wait`].
///
/// The engine sets `ready_r` to ask for readability and `ready_w` to ask for writability; the
/// platform layer clears both and sets the flags that became ready.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interest {
    /// The socket.
    pub kind: InterestKind,
    /// The query index, or `usize::MAX` for the shared UDP socket.
    pub query: usize,
    /// Whether the engine is waiting for writability rather than readability.
    pub write: bool,
    /// Set by `wait` when the socket is readable.
    pub ready_r: bool,
    /// Set by `wait` when the socket is writable.
    pub ready_w: bool,
}

impl Interest {
    /// A readable UDP interest for the shared socket.
    #[must_use]
    pub const fn udp(sock: UdpSock) -> Self {
        Self {
            kind: InterestKind::Udp(sock),
            query: usize::MAX,
            write: false,
            ready_r: false,
            ready_w: false,
        }
    }

    /// A TCP interest for one query.
    #[must_use]
    pub const fn tcp(sock: TcpSock, query: usize, write: bool) -> Self {
        Self {
            kind: InterestKind::Tcp(sock),
            query,
            write,
            ready_r: false,
            ready_w: false,
        }
    }
}

#[cfg(test)]
#[path = "types_ut.rs"]
mod tests;
