// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The iterating `getservent`/`getprotoent`/`getnetent` state machines over `FileSource`.
//!
//! There are no statics: each iterator owns its parsed state (T-12-15). Every line longer than
//! 128 bytes is skipped whole (D-12-3).

use alloc::string::String;
use alloc::vec::Vec;

use raven_libc_nss::FileSource;

use crate::consts::{DB_LINE_MAX, FILE_READ_MAX};
use crate::inet::inet_aton;
use crate::linereader::{LineReader, strip_comment, words};
use crate::netdb::{Proto, Protoent, Servent, parse_protocols};
use crate::serv::Service;

/// One network record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Netent {
    /// The name.
    pub name: String,
    /// The aliases.
    pub aliases: Vec<String>,
    /// The network number as the C integer convention.
    pub addr: u32,
}

fn parse_port_proto(text: &[u8]) -> Option<(u16, Proto)> {
    let mut parts = text.split(|byte| *byte == b'/');
    let port_text = parts.next()?;
    let proto_text = parts.next()?;
    let mut value = 0u32;
    let mut used = 0usize;
    while let Some(byte) = port_text.get(used).copied() {
        if !byte.is_ascii_digit() {
            break;
        }
        value = value
            .saturating_mul(10)
            .saturating_add(u32::from(byte.saturating_sub(b'0')));
        used = used.saturating_add(1);
    }
    if used == 0 || used != port_text.len() || value > 65535 {
        return None;
    }
    let proto = if proto_text.starts_with(b"tcp") {
        Proto::Tcp
    } else if proto_text.starts_with(b"udp") {
        Proto::Udp
    } else {
        return None;
    };
    Some((u16::try_from(value).ok()?, proto))
}

/// Parse one `/etc/services` line.
#[must_use]
pub fn parse_serv_line(line: &[u8]) -> Option<Servent> {
    let line = strip_comment(line);
    let mut tokens = words(line);
    let name = tokens.next()?;
    let port_proto = tokens.next()?;
    let (port, proto) = parse_port_proto(port_proto)?;
    let Ok(name) = core::str::from_utf8(name) else {
        return None;
    };
    let mut aliases: Vec<String> = Vec::new();
    for alias in tokens {
        if let Ok(alias) = core::str::from_utf8(alias) {
            aliases.push(String::from(alias));
        }
    }
    Some(Servent {
        name: String::from(name),
        aliases,
        port,
        proto,
    })
}

/// Parse one `/etc/networks` line.
#[must_use]
pub fn parse_net_line(line: &[u8]) -> Option<Netent> {
    let line = strip_comment(line);
    let mut tokens = words(line);
    let name = tokens.next()?;
    let address = tokens.next()?;
    let addr = inet_aton(address)?;
    let Ok(name) = core::str::from_utf8(name) else {
        return None;
    };
    let mut aliases: Vec<String> = Vec::new();
    for alias in tokens {
        if let Ok(alias) = core::str::from_utf8(alias) {
            aliases.push(String::from(alias));
        }
    }
    Some(Netent {
        name: String::from(name),
        aliases,
        addr: u32::from_ne_bytes(addr),
    })
}

/// The caller-owned `setservent`/`getservent`/`endservent` state.
pub struct ServFile {
    entries: Vec<Servent>,
    pos: usize,
}

impl ServFile {
    /// Read and parse `/etc/services` through the file seam.
    #[must_use]
    pub fn open(fs: &dyn FileSource) -> Self {
        let mut entries: Vec<Servent> = Vec::new();
        if let Ok(bytes) = fs.read_file("/etc/services", FILE_READ_MAX) {
            let mut reader = LineReader::new(&bytes, DB_LINE_MAX);
            while let Some(line) = reader.next_line() {
                if let Some(entry) = parse_serv_line(line) {
                    entries.push(entry);
                }
            }
        }
        Self { entries, pos: 0 }
    }

    /// The next entry.
    pub fn next_entry(&mut self) -> Option<&Servent> {
        let entry = self.entries.get(self.pos)?;
        self.pos = self.pos.saturating_add(1);
        Some(entry)
    }

    /// The next entry, owned.
    pub fn next_owned(&mut self) -> Option<Servent> {
        self.next_entry().cloned()
    }

    /// Restart.
    pub fn rewind(&mut self) {
        self.pos = 0;
    }

    /// Move to the end, as `endservent` does.
    pub fn finish(&mut self) {
        self.pos = self.entries.len();
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the file had no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The parsed entries.
    #[must_use]
    pub fn entries(&self) -> &[Servent] {
        &self.entries
    }

    /// Find by name or alias.
    #[must_use]
    pub fn by_name(&self, name: &str) -> Option<&Servent> {
        self.entries.iter().find(|entry| {
            entry.name.eq_ignore_ascii_case(name)
                || entry
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(name))
        })
    }

    /// Find by port and protocol.
    #[must_use]
    pub fn by_port(&self, port: u16, proto: Proto) -> Option<&Servent> {
        self.entries
            .iter()
            .find(|entry| entry.port == port && entry.proto == proto)
    }
}

/// The caller-owned `setprotoent`/`getprotoent`/`endprotoent` state.
pub struct ProtoFile {
    entries: Vec<Protoent>,
    pos: usize,
}

impl ProtoFile {
    /// Read and parse `/etc/protocols`.
    #[must_use]
    pub fn open(fs: &dyn FileSource) -> Self {
        let entries = match fs.read_file("/etc/protocols", FILE_READ_MAX) {
            Ok(bytes) => parse_protocols(&bytes),
            Err(_) => Vec::new(),
        };
        Self { entries, pos: 0 }
    }

    /// The next entry.
    pub fn next_entry(&mut self) -> Option<&Protoent> {
        let entry = self.entries.get(self.pos)?;
        self.pos = self.pos.saturating_add(1);
        Some(entry)
    }

    /// The next entry, owned.
    pub fn next_owned(&mut self) -> Option<Protoent> {
        self.next_entry().cloned()
    }

    /// Restart.
    pub fn rewind(&mut self) {
        self.pos = 0;
    }

    /// Move to the end, as `endprotoent` does.
    pub fn finish(&mut self) {
        self.pos = self.entries.len();
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the file had no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The parsed entries.
    #[must_use]
    pub fn entries(&self) -> &[Protoent] {
        &self.entries
    }
}

/// The caller-owned `setnetent`/`getnetent`/`endnetent` state.
pub struct NetFile {
    entries: Vec<Netent>,
    pos: usize,
}

impl NetFile {
    /// Read and parse `/etc/networks`.
    #[must_use]
    pub fn open(fs: &dyn FileSource) -> Self {
        let mut entries: Vec<Netent> = Vec::new();
        if let Ok(bytes) = fs.read_file("/etc/networks", FILE_READ_MAX) {
            let mut reader = LineReader::new(&bytes, DB_LINE_MAX);
            while let Some(line) = reader.next_line() {
                if let Some(entry) = parse_net_line(line) {
                    entries.push(entry);
                }
            }
        }
        Self { entries, pos: 0 }
    }

    /// The next entry.
    pub fn next_entry(&mut self) -> Option<&Netent> {
        let entry = self.entries.get(self.pos)?;
        self.pos = self.pos.saturating_add(1);
        Some(entry)
    }

    /// Restart.
    pub fn rewind(&mut self) {
        self.pos = 0;
    }

    /// Move to the end, as `endnetent` does.
    pub fn finish(&mut self) {
        self.pos = self.entries.len();
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the file had no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The parsed entries.
    #[must_use]
    pub fn entries(&self) -> &[Netent] {
        &self.entries
    }
}

/// `setservent`: open the services database.
#[must_use]
pub fn setservent(fs: &dyn FileSource) -> ServFile {
    ServFile::open(fs)
}

/// `getservent`: the next service record.
pub fn getservent(state: &mut ServFile) -> Option<Servent> {
    state.next_owned()
}

/// `endservent`: move the caller-owned state to the end.
pub fn endservent(state: &mut ServFile) {
    state.finish();
}

/// `setprotoent`: open the protocols database.
#[must_use]
pub fn setprotoent(fs: &dyn FileSource) -> ProtoFile {
    ProtoFile::open(fs)
}

/// `getprotoent`: the next protocol record.
pub fn getprotoent(state: &mut ProtoFile) -> Option<Protoent> {
    state.next_owned()
}

/// `endprotoent`: move the caller-owned state to the end.
pub fn endprotoent(state: &mut ProtoFile) {
    state.finish();
}

/// `setnetent`: open the networks database.
#[must_use]
pub fn setnetent(fs: &dyn FileSource) -> NetFile {
    NetFile::open(fs)
}

/// `getnetent`: the next network record, owned.
pub fn getnetent(state: &mut NetFile) -> Option<Netent> {
    let entry = state.next_entry()?;
    Some(entry.clone())
}

/// `endnetent`: move the caller-owned state to the end.
pub fn endnetent(state: &mut NetFile) {
    state.finish();
}

/// Convert a parsed service record into the lookup form.
#[must_use]
pub fn service_of(entry: &Servent) -> Service {
    Service {
        port: entry.port,
        proto: entry.proto.number(),
        socktype: entry.proto.socktype(),
    }
}

#[cfg(test)]
#[path = "dbfiles_ut.rs"]
mod tests;
