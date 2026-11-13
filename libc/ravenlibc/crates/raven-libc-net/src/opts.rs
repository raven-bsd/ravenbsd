// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `setsockopt`/`getsockopt` storage and the five wire options.
use crate::ErrnoExt;

use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::netinet::{IPPROTO_IPV6, IPPROTO_TCP, IPV6_V6ONLY, TCP_NODELAY};
use raven_libc_abi::socket::{
    SO_ACCEPTCONN, SO_BROADCAST, SO_DOMAIN, SO_ERROR, SO_KEEPALIVE, SO_LINGER, SO_OOBINLINE,
    SO_PROTOCOL, SO_RCVBUF, SO_RCVTIMEO, SO_REUSEADDR, SO_REUSEPORT, SO_SNDBUF, SO_SNDTIMEO,
    SO_TYPE, SOL_SOCKET,
};
use raven_proto_net::SockOpt;

/// The wire options stored until a pending socket gets a handle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WireOpts {
    /// Options in call order, so the wire sequence is deterministic.
    pub pending: Vec<(SockOpt, i32)>,
}

/// Options that have no wire id (ring 2, accepted locally).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StoredOpts {
    /// `SO_KEEPALIVE`.
    pub keepalive: i32,
    /// `SO_REUSEPORT`.
    pub reuseport: i32,
    /// `SO_LINGER` value pair.
    pub linger: [i32; 2],
    /// `SO_BROADCAST`.
    pub broadcast: i32,
    /// `SO_OOBINLINE`.
    pub oobinline: i32,
    /// `IPV6_V6ONLY`.
    pub v6only: i32,
}

/// Resolve a Linux `(level, name)` pair to a wire option.
#[must_use]
pub const fn wire_option(level: i32, name: i32) -> Option<SockOpt> {
    if level == SOL_SOCKET && name == SO_REUSEADDR {
        Some(SockOpt::ReuseAddr)
    } else if level == SOL_SOCKET && name == SO_RCVBUF {
        Some(SockOpt::RcvBuf)
    } else if level == SOL_SOCKET && name == SO_SNDBUF {
        Some(SockOpt::SndBuf)
    } else if level == IPPROTO_TCP && name == TCP_NODELAY {
        Some(SockOpt::TcpNoDelay)
    } else if level == SOL_SOCKET && name == SO_ERROR {
        Some(SockOpt::Error)
    } else {
        None
    }
}

/// Whether the option is stored locally instead of on the wire.
#[must_use]
pub const fn stored_option(level: i32, name: i32) -> bool {
    level == SOL_SOCKET
        && matches!(
            name,
            SO_KEEPALIVE | SO_REUSEPORT | SO_LINGER | SO_BROADCAST | SO_OOBINLINE
        )
        || (level == IPPROTO_IPV6 && name == IPV6_V6ONLY)
}

/// Read a little-endian `i32` from `value`, requiring exactly four bytes.
pub fn read_i32(value: &[u8]) -> Result<i32, Errno> {
    if value.len() != 4 {
        return Err(Errno::EINVAL);
    }
    let bytes = value.get(..4).ok_or(Errno::EINVAL)?;
    Ok(i32::from_le_bytes([
        *bytes.first().ok_or(Errno::EINVAL)?,
        *bytes.get(1).ok_or(Errno::EINVAL)?,
        *bytes.get(2).ok_or(Errno::EINVAL)?,
        *bytes.get(3).ok_or(Errno::EINVAL)?,
    ]))
}

/// Parse `struct linger` (two native `i32` fields).
pub fn read_linger(value: &[u8]) -> Result<[i32; 2], Errno> {
    if value.len() != 8 {
        return Err(Errno::EINVAL);
    }
    let first = read_i32(value.get(..4).ok_or(Errno::EINVAL)?)?;
    let second = read_i32(value.get(4..8).ok_or(Errno::EINVAL)?)?;
    Ok([first, second])
}

/// Parse `struct timeval` (64-bit seconds and microseconds).
pub fn read_timeval(value: &[u8]) -> Result<(i64, i64), Errno> {
    if value.len() != 16 {
        return Err(Errno::EINVAL);
    }
    let sec = read_i64(value.get(..8).ok_or(Errno::EINVAL)?)?;
    let usec = read_i64(value.get(8..16).ok_or(Errno::EINVAL)?)?;
    if sec < 0 || usec < 0 {
        return Err(Errno::EINVAL);
    }
    Ok((sec, usec))
}

fn read_i64(value: &[u8]) -> Result<i64, Errno> {
    let bytes = value.get(..8).ok_or(Errno::EINVAL)?;
    Ok(i64::from_le_bytes([
        *bytes.first().ok_or(Errno::EINVAL)?,
        *bytes.get(1).ok_or(Errno::EINVAL)?,
        *bytes.get(2).ok_or(Errno::EINVAL)?,
        *bytes.get(3).ok_or(Errno::EINVAL)?,
        *bytes.get(4).ok_or(Errno::EINVAL)?,
        *bytes.get(5).ok_or(Errno::EINVAL)?,
        *bytes.get(6).ok_or(Errno::EINVAL)?,
        *bytes.get(7).ok_or(Errno::EINVAL)?,
    ]))
}

/// Write an `i32` into `out`; returns 4 or `EINVAL` when `out` is smaller.
pub fn write_i32(out: &mut [u8], value: i32) -> Result<usize, Errno> {
    let slot = out.get_mut(..4).ok_or(Errno::EINVAL)?;
    slot.copy_from_slice(&value.to_le_bytes());
    Ok(4)
}

/// Write a `timeval`; returns 16 or `EINVAL`.
pub fn write_timeval(out: &mut [u8], sec: i64, usec: i64) -> Result<usize, Errno> {
    let slot = out.get_mut(..16).ok_or(Errno::EINVAL)?;
    slot.get_mut(..8)
        .ok_or(Errno::EINVAL)?
        .copy_from_slice(&sec.to_le_bytes());
    slot.get_mut(8..16)
        .ok_or(Errno::EINVAL)?
        .copy_from_slice(&usec.to_le_bytes());
    Ok(16)
}

/// Convert a millisecond timeout to the remaining milliseconds.
#[must_use]
pub fn timeout_ms(sec: i64, usec: i64) -> Option<u64> {
    if sec == 0 && usec == 0 {
        return None;
    }
    let millis = sec.saturating_mul(1000).saturating_add(usec / 1000);
    Some(millis.max(0) as u64)
}

/// Read a timeout option into `(sec, usec)` if it is a timeout name.
pub fn timeout_slot(name: i32) -> Option<bool> {
    if name == SO_RCVTIMEO {
        Some(true)
    } else if name == SO_SNDTIMEO {
        Some(false)
    } else {
        None
    }
}

/// Record names answered from the socket record.
pub fn record_option(name: i32) -> Option<RecordOption> {
    match name {
        SO_TYPE => Some(RecordOption::Type),
        SO_DOMAIN => Some(RecordOption::Domain),
        SO_PROTOCOL => Some(RecordOption::Protocol),
        SO_ACCEPTCONN => Some(RecordOption::AcceptConn),
        _ => None,
    }
}

/// An option answered from the record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordOption {
    /// `SO_TYPE`.
    Type,
    /// `SO_DOMAIN`.
    Domain,
    /// `SO_PROTOCOL`.
    Protocol,
    /// `SO_ACCEPTCONN`.
    AcceptConn,
}

#[cfg(test)]
#[path = "opts_ut.rs"]
mod tests;
