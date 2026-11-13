// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `gethostbyname*`/`gethostbyaddr*` wrappers and their `_r` packing.
//!
//! These are thin wrappers over [`crate::lookup::lookup_name`] and
//! [`crate::nameinfo::getnameinfo`]; a dotted quad is returned as a literal and a missing name
//! maps to the donor's `h_errno` values.

use alloc::string::String;
use alloc::vec::Vec;

use raven_libc_abi::errno::{EAGAIN, EBADMSG, EINVAL, ERANGE};

use crate::consts::{
    AF_INET, AF_INET6, AI_CANONNAME, HOST_NOT_FOUND, NO_DATA, NO_RECOVERY, TRY_AGAIN,
};
use crate::errors::Eai;
use crate::lookup::{HostOrder, lookup_name};
use crate::nameinfo::getnameinfo;
use crate::traits::Env;
use crate::types::{IpAddr, SockAddr};

/// A `hostent` in owned form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEnt {
    /// The canonical name.
    pub name: String,
    /// The aliases.
    pub aliases: Vec<String>,
    /// `AF_INET` or `AF_INET6`.
    pub addr_type: i32,
    /// The address bytes (4 or 16 each).
    pub addrs: Vec<Vec<u8>>,
}

/// A packed `hostent` inside the caller's buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedHostent {
    /// Offset of the NUL-terminated canonical name.
    pub name_off: usize,
    /// Length of the canonical name without the NUL.
    pub name_len: usize,
    /// Offset of the alias offset table.
    pub alias_table_off: usize,
    /// Number of aliases.
    pub alias_count: usize,
    /// Offset of the address offset table.
    pub addr_table_off: usize,
    /// Number of addresses.
    pub addr_count: usize,
    /// The length of every address.
    pub addr_len: usize,
    /// Total bytes used.
    pub used: usize,
}

fn herror_of(error: Eai) -> i32 {
    match error {
        Eai::NoName => HOST_NOT_FOUND,
        Eai::NoData => NO_DATA,
        Eai::Again => TRY_AGAIN,
        _ => NO_RECOVERY,
    }
}

fn errno_of(error: Eai) -> crate::Errno {
    match error {
        Eai::NoName => 0,
        Eai::NoData => 0,
        Eai::Again => EAGAIN,
        Eai::System => EAGAIN,
        _ => EBADMSG,
    }
}

fn entry_of(result: &crate::lookup::NameResult, input: &str, af: i32) -> HostEnt {
    let name = if result.canon.is_empty() {
        String::from(input)
    } else {
        result.canon.clone()
    };
    let mut aliases: Vec<String> = Vec::new();
    if name != input {
        aliases.push(String::from(input));
    }
    let mut addrs: Vec<Vec<u8>> = Vec::new();
    for address in result.addrs.iter() {
        addrs.push(address.ip().as_bytes().to_vec());
    }
    HostEnt {
        name,
        aliases,
        addr_type: af,
        addrs,
    }
}

/// `gethostbyname2`; the error is an `h_errno` value.
pub fn gethostbyname2(name: &str, af: i32, env: &Env, order: &HostOrder) -> Result<HostEnt, i32> {
    match lookup_name(Some(name), af, AI_CANONNAME, env, order) {
        Ok(result) if !result.addrs.is_empty() => Ok(entry_of(&result, name, af)),
        Ok(_) => Err(HOST_NOT_FOUND),
        Err(error) => Err(herror_of(error)),
    }
}

/// `gethostbyname`: `gethostbyname2` with `AF_INET`.
pub fn gethostbyname(name: &str, env: &Env, order: &HostOrder) -> Result<HostEnt, i32> {
    gethostbyname2(name, AF_INET, env, order)
}

/// `gethostbyaddr`; the error is an `h_errno` value.
pub fn gethostbyaddr(addr: &[u8], af: i32, env: &Env, order: &HostOrder) -> Result<HostEnt, i32> {
    let sockaddr = sockaddr_from(addr, af).ok_or(NO_RECOVERY)?;
    let (host, _) = getnameinfo(&sockaddr, 256, 0, 0, env, order).map_err(herror_of)?;
    let Some(name) = host else {
        return Err(HOST_NOT_FOUND);
    };
    Ok(HostEnt {
        name,
        aliases: Vec::new(),
        addr_type: af,
        addrs: alloc::vec![addr.to_vec()],
    })
}

fn sockaddr_from(addr: &[u8], af: i32) -> Option<SockAddr> {
    match af {
        AF_INET if addr.len() == 4 => {
            let mut bytes = [0u8; 4];
            bytes.copy_from_slice(addr);
            Some(SockAddr::new(IpAddr::V4(bytes), 0))
        }
        AF_INET6 if addr.len() == 16 => {
            let mut bytes = [0u8; 16];
            bytes.copy_from_slice(addr);
            Some(SockAddr::new(IpAddr::V6(bytes), 0))
        }
        _ => None,
    }
}

fn align4(value: usize) -> Option<usize> {
    value.checked_add(3).map(|value| value & !3usize)
}

/// Pack a `hostent`; `Err(ERANGE)` rather than a partial result.
pub fn pack_hostent(he: &HostEnt, buf: &mut [u8]) -> Result<PackedHostent, crate::Errno> {
    let mut required = he.name.len().saturating_add(1);
    for alias in he.aliases.iter() {
        required = required.saturating_add(alias.len()).saturating_add(1);
    }
    let addr_len = he.addrs.first().map_or(0, Vec::len);
    required = required.saturating_add(addr_len.saturating_mul(he.addrs.len()));
    let alias_table = align4(required).ok_or(ERANGE)?;
    let addr_table = alias_table
        .checked_add(he.aliases.len().saturating_mul(4))
        .ok_or(ERANGE)?;
    let total = addr_table
        .checked_add(he.addrs.len().saturating_mul(4))
        .ok_or(ERANGE)?;
    if buf.len() < total {
        return Err(ERANGE);
    }
    let mut cursor = 0usize;
    if let Some(slot) = buf.get_mut(cursor..cursor.saturating_add(he.name.len())) {
        slot.copy_from_slice(he.name.as_bytes());
    }
    cursor = cursor.saturating_add(he.name.len());
    if let Some(slot) = buf.get_mut(cursor) {
        *slot = 0;
    }
    cursor = cursor.saturating_add(1);
    let mut alias_offsets: Vec<u32> = Vec::new();
    for alias in he.aliases.iter() {
        alias_offsets.push(u32::try_from(cursor).map_err(|_| ERANGE)?);
        if let Some(slot) = buf.get_mut(cursor..cursor.saturating_add(alias.len())) {
            slot.copy_from_slice(alias.as_bytes());
        }
        cursor = cursor.saturating_add(alias.len());
        if let Some(slot) = buf.get_mut(cursor) {
            *slot = 0;
        }
        cursor = cursor.saturating_add(1);
    }
    let mut addr_offsets: Vec<u32> = Vec::new();
    for address in he.addrs.iter() {
        addr_offsets.push(u32::try_from(cursor).map_err(|_| ERANGE)?);
        if let Some(slot) = buf.get_mut(cursor..cursor.saturating_add(address.len())) {
            slot.copy_from_slice(address);
        }
        cursor = cursor.saturating_add(address.len());
    }
    while cursor < alias_table {
        if let Some(slot) = buf.get_mut(cursor) {
            *slot = 0;
        }
        cursor = cursor.saturating_add(1);
    }
    for (index, offset) in alias_offsets.iter().copied().enumerate() {
        let at = alias_table.saturating_add(index.saturating_mul(4));
        if let Some(slot) = buf.get_mut(at..at.saturating_add(4)) {
            slot.copy_from_slice(&offset.to_le_bytes());
        }
    }
    for (index, offset) in addr_offsets.iter().copied().enumerate() {
        let at = addr_table.saturating_add(index.saturating_mul(4));
        if let Some(slot) = buf.get_mut(at..at.saturating_add(4)) {
            slot.copy_from_slice(&offset.to_le_bytes());
        }
    }
    Ok(PackedHostent {
        name_off: 0,
        name_len: he.name.len(),
        alias_table_off: alias_table,
        alias_count: he.aliases.len(),
        addr_table_off: addr_table,
        addr_count: he.addrs.len(),
        addr_len,
        used: total,
    })
}

/// `gethostbyname2_r`: `Ok(None)` for not found, `Err(ERANGE)` when the buffer is too small.
pub fn gethostbyname2_r(
    name: &str,
    af: i32,
    buf: &mut [u8],
    env: &Env,
    order: &HostOrder,
) -> Result<Option<PackedHostent>, crate::Errno> {
    match lookup_name(Some(name), af, AI_CANONNAME, env, order) {
        Ok(result) if !result.addrs.is_empty() => {
            let entry = entry_of(&result, name, af);
            pack_hostent(&entry, buf).map(Some)
        }
        Ok(_) => Ok(None),
        Err(Eai::NoName) | Err(Eai::NoData) => Ok(None),
        Err(error) => Err(errno_of(error)),
    }
}

/// `gethostbyname_r`.
pub fn gethostbyname_r(
    name: &str,
    buf: &mut [u8],
    env: &Env,
    order: &HostOrder,
) -> Result<Option<PackedHostent>, crate::Errno> {
    gethostbyname2_r(name, AF_INET, buf, env, order)
}

/// `gethostbyaddr_r`.
pub fn gethostbyaddr_r(
    addr: &[u8],
    af: i32,
    buf: &mut [u8],
    env: &Env,
    order: &HostOrder,
) -> Result<Option<PackedHostent>, crate::Errno> {
    let Some(sockaddr) = sockaddr_from(addr, af) else {
        return Err(EINVAL);
    };
    let (host, _) = match getnameinfo(&sockaddr, 256, 0, 0, env, order) {
        Ok(value) => value,
        Err(Eai::NoName) | Err(Eai::NoData) => return Ok(None),
        Err(Eai::Again) => return Err(EAGAIN),
        Err(_) => return Err(EBADMSG),
    };
    let Some(name) = host else {
        return Ok(None);
    };
    let entry = HostEnt {
        name,
        aliases: Vec::new(),
        addr_type: af,
        addrs: alloc::vec![addr.to_vec()],
    };
    pack_hostent(&entry, buf).map(Some)
}

/// The `h_errno` a legacy wrapper reports for an [`Eai`].
#[must_use]
pub fn herror_for(error: Eai) -> i32 {
    herror_of(error)
}

#[cfg(test)]
#[path = "legacy_ut.rs"]
mod tests;
