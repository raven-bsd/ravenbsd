// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `servent`/`protoent` lookups, the built-in protocol table (D-12-1) and the `_r` packing.

use alloc::string::String;
use alloc::vec::Vec;

use raven_libc_abi::errno::{EINVAL, ENOENT, ENOMEM, ERANGE};
use raven_libc_nss::FileSource;

use crate::consts::{DB_LINE_MAX, IPPROTO_TCP, IPPROTO_UDP, SOCK_DGRAM, SOCK_STREAM};
use crate::errors::Eai;
use crate::linereader::{LineReader, strip_comment, words};
use crate::serv::lookup_serv;

/// The transport protocol of a service record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Proto {
    /// `tcp`.
    Tcp,
    /// `udp`.
    Udp,
}

impl Proto {
    /// The protocol name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
        }
    }

    /// The IP protocol number.
    #[must_use]
    pub const fn number(self) -> i32 {
        match self {
            Self::Tcp => IPPROTO_TCP,
            Self::Udp => IPPROTO_UDP,
        }
    }

    /// The socket type.
    #[must_use]
    pub const fn socktype(self) -> i32 {
        match self {
            Self::Tcp => SOCK_STREAM,
            Self::Udp => SOCK_DGRAM,
        }
    }

    /// Parse `tcp` or `udp`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("tcp") {
            Some(Self::Tcp)
        } else if name.eq_ignore_ascii_case("udp") {
            Some(Self::Udp)
        } else {
            None
        }
    }

    /// Parse an IP protocol number.
    #[must_use]
    pub const fn from_number(number: i32) -> Option<Self> {
        match number {
            IPPROTO_TCP => Some(Self::Tcp),
            IPPROTO_UDP => Some(Self::Udp),
            _ => None,
        }
    }
}

/// One service record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Servent {
    /// The name.
    pub name: String,
    /// The aliases (may include the name in the `getservbyname` form).
    pub aliases: Vec<String>,
    /// The port.
    pub port: u16,
    /// The protocol.
    pub proto: Proto,
}

/// One protocol record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Protoent {
    /// The name.
    pub name: String,
    /// The aliases.
    pub aliases: Vec<String>,
    /// The protocol number.
    pub proto: u8,
}

/// The built-in protocol table (musl `proto.c`, D-12-1 fallback).
pub static BUILTIN_PROTOCOLS: [(&str, u8); 35] = [
    ("ip", 0),
    ("icmp", 1),
    ("igmp", 2),
    ("ggp", 3),
    ("ipencap", 4),
    ("st", 5),
    ("tcp", 6),
    ("egp", 8),
    ("pup", 12),
    ("udp", 17),
    ("hmp", 20),
    ("xns-idp", 22),
    ("rdp", 27),
    ("iso-tp4", 29),
    ("xtp", 36),
    ("ddp", 37),
    ("idpr-cmtp", 38),
    ("ipv6", 41),
    ("ipv6-route", 43),
    ("ipv6-frag", 44),
    ("idrp", 45),
    ("rsvp", 46),
    ("gre", 47),
    ("esp", 50),
    ("ah", 51),
    ("skip", 57),
    ("ipv6-icmp", 58),
    ("ipv6-nonxt", 59),
    ("ipv6-opts", 60),
    ("rspf", 73),
    ("vmtp", 81),
    ("ospf", 89),
    ("ipip", 94),
    ("encap", 98),
    ("pim", 103),
];

fn parse_number(text: &[u8]) -> Option<u8> {
    let mut value = 0u16;
    let mut used = 0usize;
    while let Some(byte) = text.get(used).copied() {
        if !byte.is_ascii_digit() {
            break;
        }
        value = value
            .saturating_mul(10)
            .saturating_add(u16::from(byte.saturating_sub(b'0')));
        used = used.saturating_add(1);
    }
    if used == 0 || value > 255 {
        None
    } else {
        u8::try_from(value).ok()
    }
}

/// Parse a `/etc/protocols` file.
#[must_use]
pub fn parse_protocols(file: &[u8]) -> Vec<Protoent> {
    let mut out = Vec::new();
    let mut reader = LineReader::new(file, DB_LINE_MAX);
    while let Some(line) = reader.next_line() {
        let line = strip_comment(line);
        let mut tokens = words(line);
        let Some(name) = tokens.next() else {
            continue;
        };
        let Some(number) = tokens.next() else {
            continue;
        };
        let Some(proto) = parse_number(number) else {
            continue;
        };
        let Ok(name) = core::str::from_utf8(name) else {
            continue;
        };
        let mut aliases: Vec<String> = Vec::new();
        for alias in tokens {
            if let Ok(alias) = core::str::from_utf8(alias) {
                aliases.push(String::from(alias));
            }
        }
        out.push(Protoent {
            name: String::from(name),
            aliases,
            proto,
        });
    }
    out
}

fn protocol_entry(name: &str, proto: u8) -> Protoent {
    Protoent {
        name: String::from(name),
        aliases: Vec::new(),
        proto,
    }
}

/// `getprotobyname`: `/etc/protocols` first (D-12-1), then the built-in table.
pub fn getprotobyname(fs: &dyn FileSource, name: &str) -> Result<Option<Protoent>, crate::Errno> {
    let file = fs.read_file("/etc/protocols", crate::consts::FILE_READ_MAX);
    if let Ok(bytes) = file {
        for entry in parse_protocols(&bytes) {
            if entry.name.eq_ignore_ascii_case(name)
                || entry
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(name))
            {
                return Ok(Some(entry));
            }
        }
    }
    for (builtin, number) in BUILTIN_PROTOCOLS.iter().copied() {
        if builtin.eq_ignore_ascii_case(name) {
            return Ok(Some(protocol_entry(builtin, number)));
        }
    }
    Ok(None)
}

/// `getprotobynumber`: `/etc/protocols` first (D-12-1), then the built-in table.
pub fn getprotobynumber(fs: &dyn FileSource, number: u8) -> Result<Option<Protoent>, crate::Errno> {
    let file = fs.read_file("/etc/protocols", crate::consts::FILE_READ_MAX);
    if let Ok(bytes) = file {
        for entry in parse_protocols(&bytes) {
            if entry.proto == number {
                return Ok(Some(entry));
            }
        }
    }
    for (builtin, value) in BUILTIN_PROTOCOLS.iter().copied() {
        if value == number {
            return Ok(Some(protocol_entry(builtin, value)));
        }
    }
    Ok(None)
}

fn numeric_name(name: &[u8]) -> bool {
    !name.is_empty() && name.iter().all(|byte| byte.is_ascii_digit())
}

/// `getservbyname_r`: `Err(ENOENT)` for a numeric name (not a service record).
pub fn getservbyname(
    fs: &dyn FileSource,
    name: &str,
    proto: Option<&str>,
) -> Result<Option<Servent>, crate::Errno> {
    if numeric_name(name.as_bytes()) {
        return Err(ENOENT);
    }
    let proto = match proto {
        None => None,
        Some(text) => match Proto::from_name(text) {
            Some(value) => Some(value),
            None => return Err(EINVAL),
        },
    };
    let services =
        lookup_serv(fs, Some(name), proto.map_or(0, Proto::number), 0, 0).map_err(|error| {
            match error {
                Eai::Memory => ENOMEM,
                Eai::System => ENOMEM,
                _ => ENOENT,
            }
        })?;
    let Some(first) = services.first().copied() else {
        return Err(ENOENT);
    };
    let Some(proto) = Proto::from_number(first.proto) else {
        return Err(ENOENT);
    };
    Ok(Some(Servent {
        name: String::from(name),
        aliases: alloc::vec![String::from(name)],
        port: first.port,
        proto,
    }))
}

/// The canonical service name of a port from `/etc/services`.
#[must_use]
pub fn service_name(fs: &dyn FileSource, port: u16, proto: Proto) -> Option<String> {
    let file = fs
        .read_file("/etc/services", crate::consts::FILE_READ_MAX)
        .ok()?;
    let mut reader = LineReader::new(&file, DB_LINE_MAX);
    while let Some(line) = reader.next_line() {
        let line = strip_comment(line);
        let mut tokens = words(line);
        let Some(name) = tokens.next() else {
            continue;
        };
        let Some(port_proto) = tokens.next() else {
            continue;
        };
        let mut parts = port_proto.split(|byte| *byte == b'/');
        let Some(port_text) = parts.next() else {
            continue;
        };
        let Some(proto_text) = parts.next() else {
            continue;
        };
        let Ok(port_text) = core::str::from_utf8(port_text) else {
            continue;
        };
        let Ok(value) = port_text.parse::<u32>() else {
            continue;
        };
        if value != u32::from(port) || !proto_text.starts_with(proto.name().as_bytes()) {
            continue;
        }
        let Ok(name) = core::str::from_utf8(name) else {
            continue;
        };
        if !name.is_empty() && name.len() <= 32 {
            return Some(String::from(name));
        }
    }
    None
}

/// `getservbyport_r`.
pub fn getservbyport(
    fs: &dyn FileSource,
    port: u16,
    proto: Option<&str>,
) -> Result<Option<Servent>, crate::Errno> {
    let proto = match proto {
        None => {
            if let Ok(Some(entry)) = getservbyport(fs, port, Some("tcp")) {
                return Ok(Some(entry));
            }
            return getservbyport(fs, port, Some("udp"));
        }
        Some(text) => match Proto::from_name(text) {
            Some(value) => value,
            None => return Err(EINVAL),
        },
    };
    let Some(name) = service_name(fs, port, proto) else {
        return Err(ENOENT);
    };
    if name.as_bytes().iter().all(|byte| byte.is_ascii_digit()) {
        let Ok(value) = name.parse::<u32>() else {
            return Err(ENOENT);
        };
        if value == u32::from(port) {
            return Err(ENOENT);
        }
    }
    Ok(Some(Servent {
        name: name.clone(),
        aliases: alloc::vec![name],
        port,
        proto,
    }))
}

/// A packed `servent` inside the caller's buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedServent {
    /// Offset of the NUL-terminated name.
    pub name_off: usize,
    /// Length of the name without the NUL.
    pub name_len: usize,
    /// Offset of the alias offset table.
    pub alias_table_off: usize,
    /// Number of aliases.
    pub alias_count: usize,
    /// The port.
    pub port: u16,
    /// The protocol number.
    pub proto: i32,
    /// Total bytes used.
    pub used: usize,
}

/// A packed `protoent` inside the caller's buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedProtoent {
    /// Offset of the NUL-terminated name.
    pub name_off: usize,
    /// Length of the name without the NUL.
    pub name_len: usize,
    /// Offset of the alias offset table.
    pub alias_table_off: usize,
    /// Number of aliases.
    pub alias_count: usize,
    /// The protocol number.
    pub proto: u8,
    /// Total bytes used.
    pub used: usize,
}

fn align4(value: usize) -> Option<usize> {
    value.checked_add(3).map(|value| value & !3usize)
}

fn pack_strings(
    name: &str,
    aliases: &[String],
    buf: &mut [u8],
) -> Result<(usize, usize, usize, usize, usize), crate::Errno> {
    let mut required = name.len().saturating_add(1);
    for alias in aliases {
        required = required.saturating_add(alias.len()).saturating_add(1);
    }
    let table = align4(required).ok_or(ERANGE)?;
    let total = table
        .checked_add(aliases.len().saturating_mul(4))
        .ok_or(ERANGE)?;
    if buf.len() < total {
        return Err(ERANGE);
    }
    let mut cursor = 0usize;
    if let Some(slot) = buf.get_mut(cursor..cursor.saturating_add(name.len())) {
        slot.copy_from_slice(name.as_bytes());
    }
    cursor = cursor.saturating_add(name.len());
    if let Some(slot) = buf.get_mut(cursor) {
        *slot = 0;
    }
    cursor = cursor.saturating_add(1);
    let mut offsets: Vec<u32> = Vec::with_capacity(aliases.len());
    for alias in aliases {
        offsets.push(u32::try_from(cursor).map_err(|_| ERANGE)?);
        if let Some(slot) = buf.get_mut(cursor..cursor.saturating_add(alias.len())) {
            slot.copy_from_slice(alias.as_bytes());
        }
        cursor = cursor.saturating_add(alias.len());
        if let Some(slot) = buf.get_mut(cursor) {
            *slot = 0;
        }
        cursor = cursor.saturating_add(1);
    }
    while cursor < table {
        if let Some(slot) = buf.get_mut(cursor) {
            *slot = 0;
        }
        cursor = cursor.saturating_add(1);
    }
    for (index, offset) in offsets.iter().copied().enumerate() {
        let at = table.saturating_add(index.saturating_mul(4));
        let bytes = offset.to_le_bytes();
        if let Some(slot) = buf.get_mut(at..at.saturating_add(4)) {
            slot.copy_from_slice(&bytes);
        }
    }
    Ok((0, name.len(), table, aliases.len(), total))
}

/// Pack a `servent`; `Err(ERANGE)` rather than a partial result.
pub fn pack_servent(se: &Servent, buf: &mut [u8]) -> Result<PackedServent, crate::Errno> {
    let (name_off, name_len, alias_table_off, alias_count, used) =
        pack_strings(&se.name, &se.aliases, buf)?;
    Ok(PackedServent {
        name_off,
        name_len,
        alias_table_off,
        alias_count,
        port: se.port,
        proto: se.proto.number(),
        used,
    })
}

/// Pack a `protoent`; `Err(ERANGE)` rather than a partial result.
pub fn pack_protoent(pe: &Protoent, buf: &mut [u8]) -> Result<PackedProtoent, crate::Errno> {
    let (name_off, name_len, alias_table_off, alias_count, used) =
        pack_strings(&pe.name, &pe.aliases, buf)?;
    Ok(PackedProtoent {
        name_off,
        name_len,
        alias_table_off,
        alias_count,
        proto: pe.proto,
        used,
    })
}

/// The protocol number of a built-in name.
#[must_use]
pub fn builtin_protocol_number(name: &str) -> Option<u8> {
    BUILTIN_PROTOCOLS
        .iter()
        .find(|(builtin, _)| builtin.eq_ignore_ascii_case(name))
        .map(|(_, number)| *number)
}

#[cfg(test)]
#[path = "netdb_ut.rs"]
mod tests;
