// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `if_*` and `getifaddrs` over the `net:` interface list.
use crate::ErrnoExt;

use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::misc::IF_NAMESIZE;
use raven_proto_net::{IF_LINK_UP, IF_LOOPBACK, IF_UP, IfaceInfo, IfaceResponse};

use crate::NetBackend;

/// Interface name buffer size, including the NUL.
pub const MAX_IFNAME: usize = IF_NAMESIZE as usize;

/// `IFF_UP` from `libc/ravenlibc/include/net/if.h`.
pub const IFF_UP: u32 = 0x1;
/// `IFF_BROADCAST`.
pub const IFF_BROADCAST: u32 = 0x2;
/// `IFF_LOOPBACK`.
pub const IFF_LOOPBACK: u32 = 0x8;
/// `IFF_RUNNING`.
pub const IFF_RUNNING: u32 = 0x40;
/// `IFF_MULTICAST`.
pub const IFF_MULTICAST: u32 = 0x1000;

/// One entry returned by `getifaddrs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IfAddr {
    /// Interface name.
    pub name: Vec<u8>,
    /// Linux `IFF_*` flags.
    pub flags: u32,
    /// IPv4 address octets.
    pub addr: [u8; 4],
    /// Dotted netmask octets.
    pub netmask: [u8; 4],
    /// Broadcast address when the interface is not point-to-point or loopback.
    pub broadaddr: Option<[u8; 4]>,
}

/// Read the complete interface list from a backend.
pub fn list(backend: &dyn NetBackend) -> Result<Vec<IfaceInfo>, Errno> {
    let responses = backend.iface_list()?;
    let mut out = Vec::new();
    let mut expected: Option<usize> = None;
    for response in responses {
        match response {
            IfaceResponse::Count { count } => {
                expected = Some(usize::from(count));
            }
            IfaceResponse::Iface(info) => out.push(info),
            IfaceResponse::Err(error) => return Err(Errno(error.0)),
            _ => return Err(Errno::EPROTO),
        }
    }
    if let Some(expected) = expected
        && expected != out.len()
    {
        return Err(Errno::EPROTO);
    }
    Ok(out)
}

/// `if_nametoindex`.
pub fn nametoindex(backend: &dyn NetBackend, name: &[u8]) -> Result<u32, Errno> {
    if name.is_empty() || name.len() >= MAX_IFNAME {
        return Err(Errno::ENODEV);
    }
    let list = list(backend)?;
    for (position, info) in list.iter().enumerate() {
        if info.name.as_str().as_bytes() == name {
            return Ok(index_of(backend, info, position));
        }
    }
    Err(Errno::ENODEV)
}

/// `if_indextoname`; writes the NUL-terminated name into the 16-byte buffer.
pub fn indextoname(
    backend: &dyn NetBackend,
    idx: u32,
    out: &mut [u8; MAX_IFNAME],
) -> Result<usize, Errno> {
    if idx == 0 {
        return Err(Errno::ENXIO);
    }
    let list = list(backend)?;
    for (position, info) in list.iter().enumerate() {
        if index_of(backend, info, position) == idx {
            let name = info.name.as_str().as_bytes();
            if name.len() >= MAX_IFNAME {
                return Err(Errno::ENXIO);
            }
            let length = name.len();
            if let Some(slot) = out.get_mut(..length) {
                slot.copy_from_slice(name);
            }
            if let Some(term) = out.get_mut(length) {
                *term = 0;
            }
            return Ok(length);
        }
    }
    Err(Errno::ENXIO)
}

/// `if_nameindex`: `(index, name)` in list order.
pub fn nameindex(backend: &dyn NetBackend) -> Result<Vec<(u32, Vec<u8>)>, Errno> {
    let list = list(backend)?;
    let mut out = Vec::new();
    for (position, info) in list.iter().enumerate() {
        out.push((
            index_of(backend, info, position),
            info.name.as_str().as_bytes().to_vec(),
        ));
    }
    Ok(out)
}

/// `getifaddrs`; one IPv4 entry per interface.
pub fn getifaddrs(backend: &dyn NetBackend) -> Result<Vec<IfAddr>, Errno> {
    let list = list(backend)?;
    let mut out = Vec::new();
    for info in list.iter() {
        let loopback = info.flags & IF_LOOPBACK != 0;
        let flags = (if info.flags & IF_UP != 0 {
            IFF_UP
                | if info.flags & IF_LINK_UP != 0 {
                    IFF_RUNNING
                } else {
                    0
                }
        } else {
            0
        }) | if loopback {
            IFF_LOOPBACK
        } else {
            IFF_BROADCAST | IFF_MULTICAST
        };
        let netmask = prefix_mask(info.prefix);
        let broadaddr = if !loopback && info.prefix <= 30 {
            Some(broadcast(info.addr, netmask))
        } else {
            None
        };
        out.push(IfAddr {
            name: info.name.as_str().as_bytes().to_vec(),
            flags,
            addr: info.addr,
            netmask,
            broadaddr,
        });
    }
    Ok(out)
}

fn index_of(_backend: &dyn NetBackend, _info: &IfaceInfo, position: usize) -> u32 {
    u32::try_from(position.saturating_add(1)).unwrap_or(1)
}

/// The dotted mask for a prefix length.
#[must_use]
pub fn prefix_mask(prefix: u8) -> [u8; 4] {
    let prefix = prefix.min(32);
    let mask = if prefix == 0 {
        0u32
    } else {
        u32::MAX << (32 - u32::from(prefix))
    };
    mask.to_be_bytes()
}

/// `addr | ~mask`.
#[must_use]
pub fn broadcast(addr: [u8; 4], mask: [u8; 4]) -> [u8; 4] {
    [
        addr[0] | !mask[0],
        addr[1] | !mask[1],
        addr[2] | !mask[2],
        addr[3] | !mask[3],
    ]
}

#[cfg(test)]
#[path = "iface_ut.rs"]
mod tests;
