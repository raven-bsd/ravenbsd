// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `/etc/hosts` grammar and the hosts backend (clean-room, A-net-6, D-12-3, D-12-6).

use alloc::string::String;
use alloc::vec::Vec;

use crate::consts::{AF_UNSPEC, HOSTS_LINE_MAX, MAXADDRS};
use crate::linereader::{LineReader, strip_comment, words};
use crate::literal::lookup_ipliteral;
use crate::lookup::Address;
use crate::name::is_valid_hostname;
use crate::traits::IfNames;
use crate::types::{Family, IpAddr};

/// One parsed hosts line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostLine {
    /// The address.
    pub addr: IpAddr,
    /// The IPv6 scope id.
    pub scope: u32,
    /// The canonical name followed by the aliases.
    pub names: Vec<String>,
}

impl HostLine {
    /// The canonical name, when the line has one.
    #[must_use]
    pub fn canonical(&self) -> Option<&str> {
        self.names.first().map(String::as_str)
    }
}

/// Parse one hosts line; `#` starts a comment, blanks separate tokens.
#[must_use]
pub fn parse_hosts_line(line: &[u8], ifnames: &dyn IfNames) -> Option<HostLine> {
    let line = strip_comment(line);
    let mut tokens = words(line);
    let address = tokens.next()?;
    let literal = lookup_ipliteral(address, AF_UNSPEC, ifnames).ok()??;
    let mut names: Vec<String> = Vec::new();
    for token in tokens {
        if let Ok(text) = core::str::from_utf8(token) {
            names.push(String::from(text));
        }
    }
    if names.is_empty() {
        return None;
    }
    Some(HostLine {
        addr: literal.ip(),
        scope: literal.scope,
        names,
    })
}

/// Parse a whole hosts file, skipping over-long and malformed lines whole.
#[must_use]
pub fn parse_hosts(file: &[u8], ifnames: &dyn IfNames) -> Vec<HostLine> {
    let mut out = Vec::new();
    let mut reader = LineReader::new(file, HOSTS_LINE_MAX);
    while let Some(line) = reader.next_line() {
        if let Some(entry) = parse_hosts_line(line, ifnames) {
            out.push(entry);
        }
    }
    out
}

/// The `gethostent` iterator over a hosts file.
pub struct HostsIter<'a> {
    reader: LineReader<'a>,
    ifnames: &'a dyn IfNames,
}

impl<'a> HostsIter<'a> {
    /// Iterate `data`.
    #[must_use]
    pub const fn new(data: &'a [u8], ifnames: &'a dyn IfNames) -> Self {
        Self {
            reader: LineReader::new(data, HOSTS_LINE_MAX),
            ifnames,
        }
    }

    /// The next parsed entry.
    pub fn next_entry(&mut self) -> Option<HostLine> {
        while let Some(line) = self.reader.next_line() {
            if let Some(entry) = parse_hosts_line(line, self.ifnames) {
                return Some(entry);
            }
        }
        None
    }

    /// Restart at the first line.
    pub fn rewind(&mut self) {
        self.reader.rewind();
    }

    /// Consume the remaining lines, as `endhostent` does.
    pub fn finish(&mut self) {
        while self.reader.next_line().is_some() {}
    }
}

fn matches_name(names: &[String], wanted: &[u8]) -> bool {
    names
        .iter()
        .any(|name| name.as_bytes().eq_ignore_ascii_case(wanted))
}

/// The hosts backend: all matching lines up to `MAXADDRS`, first valid canonical name.
pub fn lookup(
    file: &[u8],
    wanted: &[u8],
    family: i32,
    ifnames: &dyn IfNames,
    canon: &mut String,
) -> Result<(Vec<Address>, bool), crate::Errno> {
    let mut addrs: Vec<Address> = Vec::new();
    let mut badfam = false;
    let mut have_canon = false;
    let mut reader = LineReader::new(file, HOSTS_LINE_MAX);
    while let Some(line) = reader.next_line() {
        if addrs.len() >= MAXADDRS {
            break;
        }
        let Some(entry) = parse_hosts_line(line, ifnames) else {
            continue;
        };
        if !matches_name(&entry.names, wanted) {
            continue;
        }
        let entry_family = entry.addr.family();
        if family != AF_UNSPEC && family != entry_family.af() {
            badfam = true;
        } else {
            let mut address = Address::new(entry_family);
            address.addr = match entry.addr {
                IpAddr::V4(bytes) => IpAddr::V4(bytes).to_v4_mapped().octets16(),
                IpAddr::V6(bytes) => bytes,
            };
            address.scope = entry.scope;
            addrs.push(address);
        }
        if !have_canon
            && let Some(name) = entry.canonical()
            && is_valid_hostname(name)
        {
            canon.clear();
            canon.push_str(name);
            have_canon = true;
        }
    }
    Ok((addrs, badfam))
}

/// `sethostent`: start the caller-owned hosts iterator.
#[must_use]
pub fn sethostent<'a>(data: &'a [u8], ifnames: &'a dyn IfNames) -> HostsIter<'a> {
    HostsIter::new(data, ifnames)
}

/// `gethostent`: the next hosts entry.
pub fn gethostent(state: &mut HostsIter<'_>) -> Option<HostLine> {
    state.next_entry()
}

/// `endhostent`: consume the rest of the caller-owned iterator.
pub fn endhostent(state: &mut HostsIter<'_>) {
    state.finish();
}

/// The canonical name of the first hosts line matching `wanted`.
#[must_use]
pub fn canonical_name(file: &[u8], wanted: &[u8], ifnames: &dyn IfNames) -> Option<String> {
    let mut entries = HostsIter::new(file, ifnames);
    while let Some(entry) = entries.next_entry() {
        if matches_name(&entry.names, wanted) {
            return entry.canonical().map(String::from);
        }
    }
    None
}

/// Addresses of all matching hosts lines, ignoring family.
#[must_use]
pub fn all_addresses(file: &[u8], wanted: &[u8], ifnames: &dyn IfNames) -> Vec<IpAddr> {
    let mut out = Vec::new();
    let mut entries = HostsIter::new(file, ifnames);
    while let Some(entry) = entries.next_entry() {
        if matches_name(&entry.names, wanted) {
            out.push(entry.addr);
            if out.len() >= MAXADDRS {
                break;
            }
        }
    }
    out
}

/// The family of a hosts line address.
#[must_use]
pub const fn family_of(addr: &IpAddr) -> Family {
    addr.family()
}

#[cfg(test)]
#[path = "hosts_ut.rs"]
mod tests;
