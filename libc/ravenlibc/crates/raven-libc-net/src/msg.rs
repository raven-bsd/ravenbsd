// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Iovec gather/scatter and `net:` path construction.
use crate::ErrnoExt;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::socket::{SOCK_DGRAM, SOCK_STREAM};
use raven_proto_net::{Endpoint, MAX_DATAGRAM};

use crate::itoa;

/// Total byte length of an iovec list.
#[must_use]
pub fn total_len(iov: &[&[u8]]) -> usize {
    iov.iter()
        .fold(0usize, |sum, part| sum.saturating_add(part.len()))
}

/// Copy every iovec into `out`.
pub fn gather(iov: &[&[u8]], out: &mut [u8]) -> Result<usize, Errno> {
    let total = total_len(iov);
    if total > MAX_DATAGRAM {
        return Err(Errno::EMSGSIZE);
    }
    if out.len() < total {
        return Err(Errno::EMSGSIZE);
    }
    let mut position = 0usize;
    for part in iov {
        let end = position.saturating_add(part.len());
        let target = out.get_mut(position..end).ok_or(Errno::EFAULT)?;
        target.copy_from_slice(part);
        position = end;
    }
    Ok(position)
}

/// Copy `data` across the iovecs in order; returns the bytes placed.
pub fn scatter(data: &[u8], iov: &mut [&mut [u8]]) -> usize {
    let mut position = 0usize;
    for part in iov.iter_mut() {
        if position >= data.len() {
            break;
        }
        let count = part.len().min(data.len().saturating_sub(position));
        let end = position.saturating_add(count);
        if let (Some(target), Some(source)) = (part.get_mut(..count), data.get(position..end)) {
            target.copy_from_slice(source);
        }
        position = end;
    }
    position
}

/// Build `net:tcp/<ipv4>:<port>` or `net:udp/...`.
pub fn socket_path(ty: i32, endpoint: Endpoint, out: &mut [u8]) -> Result<&str, Errno> {
    let prefix: &[u8] = if ty == SOCK_STREAM {
        b"net:tcp/"
    } else if ty == SOCK_DGRAM {
        b"net:udp/"
    } else {
        return Err(Errno::ESOCKTNOSUPPORT);
    };
    if endpoint.port == 0 {
        return Err(Errno::ECONNREFUSED);
    }
    let mut position = 0usize;
    let slot = out.get_mut(..prefix.len()).ok_or(Errno::ENOBUFS)?;
    slot.copy_from_slice(prefix);
    position = position.saturating_add(prefix.len());
    let address = itoa::ipv4(endpoint.ip, out.get_mut(position..).ok_or(Errno::ENOBUFS)?)
        .ok_or(Errno::ENOBUFS)?;
    position = position.saturating_add(address.len());
    let colon = out.get_mut(position).ok_or(Errno::ENOBUFS)?;
    *colon = b':';
    position = position.saturating_add(1);
    let port = itoa::decimal(
        u32::from(endpoint.port),
        out.get_mut(position..).ok_or(Errno::ENOBUFS)?,
    )
    .ok_or(Errno::ENOBUFS)?;
    position = position.saturating_add(port.len());
    let bytes = out.get(..position).ok_or(Errno::ENOBUFS)?;
    core::str::from_utf8(bytes).map_err(|_| Errno::EINVAL)
}

/// Build a bare `net:tcp` or `net:udp` path.
pub fn bare_path(ty: i32, out: &mut [u8]) -> Result<&str, Errno> {
    let text: &[u8] = if ty == SOCK_STREAM {
        b"net:tcp"
    } else if ty == SOCK_DGRAM {
        b"net:udp"
    } else {
        return Err(Errno::ESOCKTNOSUPPORT);
    };
    let slot = out.get_mut(..text.len()).ok_or(Errno::ENOBUFS)?;
    slot.copy_from_slice(text);
    core::str::from_utf8(slot).map_err(|_| Errno::EINVAL)
}

#[cfg(test)]
#[path = "msg_ut.rs"]
mod tests;
