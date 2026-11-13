// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The closed-set `nsswitch.conf` model and the shared line scanner.
//!
//! NSS plugins are never loaded: the only sources are `files` and `dns`, and every other name
//! (nis, ldap, compat, mdns, resolve, sss, winbind, ...) is a recorded `Skipped` entry.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )
)] // tests use fixed vectors and fail by panicking

extern crate alloc;

use alloc::vec::Vec;

/// The error type used by the file seam (the ABI's positive Linux number).
pub type Errno = i32;

/// The maximum accepted `nsswitch.conf` line.
pub const MAX_LINE: usize = 4096;

/// The maximum entries in one source list.
pub const MAX_SOURCES: usize = 8;

/// A file reader supplied by the platform layer.
pub trait FileSource {
    /// Read at most `max` bytes from `path`.
    fn read_file(&self, path: &str, max: usize) -> Result<Vec<u8>, Errno>;
}

/// One nsswitch database.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Database {
    /// `hosts`.
    Hosts,
    /// `passwd`.
    Passwd,
    /// `group`.
    Group,
    /// `services`.
    Services,
    /// `protocols`.
    Protocols,
    /// `networks`.
    Networks,
}

/// One source in a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The local files.
    Files,
    /// The DNS resolver.
    Dns,
    /// A source that is recorded but never called.
    Skipped,
}

/// The result of one source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The entry was found.
    Success,
    /// The source has no such entry.
    NotFound,
    /// The source is temporarily unavailable.
    Unavail,
    /// The source asks to try again.
    TryAgain,
}

/// One source and its action flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceEntry {
    /// The source.
    pub source: Source,
    /// `[NOTFOUND=return]` follows this source.
    pub on_notfound_return: bool,
}

/// A fixed-capacity source list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceList {
    entries: [Option<SourceEntry>; MAX_SOURCES],
    len: usize,
}

impl SourceList {
    /// An empty list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: [None; MAX_SOURCES],
            len: 0,
        }
    }

    fn push(&mut self, entry: SourceEntry) {
        if let Some(slot) = self.entries.get_mut(self.len) {
            *slot = Some(entry);
            self.len = self.len.saturating_add(1);
        }
    }

    fn set_last_notfound(&mut self) {
        if self.len == 0 {
            return;
        }
        if let Some(Some(entry)) = self.entries.get_mut(self.len.saturating_sub(1)) {
            entry.on_notfound_return = true;
        }
    }

    /// The entries in order.
    pub fn iter(&self) -> impl Iterator<Item = SourceEntry> + '_ {
        self.entries.iter().take(self.len).flatten().copied()
    }

    /// Number of entries.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the list is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The entry at `index`.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<SourceEntry> {
        self.entries.get(index).copied().flatten()
    }
}

impl Default for SourceList {
    fn default() -> Self {
        Self::new()
    }
}

/// The parsed `nsswitch.conf`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nsswitch {
    hosts: SourceList,
    passwd: SourceList,
    group: SourceList,
    services: SourceList,
    protocols: SourceList,
    networks: SourceList,
}

impl Nsswitch {
    /// The built-in defaults: hosts `files dns`, everything else `files`.
    #[must_use]
    pub fn defaults() -> Self {
        let mut value = Self::empty();
        value.hosts = list(&[
            SourceEntry {
                source: Source::Files,
                on_notfound_return: false,
            },
            SourceEntry {
                source: Source::Dns,
                on_notfound_return: false,
            },
        ]);
        value.passwd = files_only();
        value.group = files_only();
        value.services = files_only();
        value.protocols = files_only();
        value.networks = files_only();
        value
    }

    /// An empty configuration.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            hosts: SourceList::new(),
            passwd: SourceList::new(),
            group: SourceList::new(),
            services: SourceList::new(),
            protocols: SourceList::new(),
            networks: SourceList::new(),
        }
    }

    /// Parse a configuration file. A malformed non-comment line gives the defaults.
    #[must_use]
    pub fn parse(file: &[u8]) -> Self {
        let mut value = Self::empty();
        for line in file.split(|byte| *byte == b'\n') {
            if line.len() > MAX_LINE {
                continue;
            }
            let line = line.split(|byte| *byte == b'#').next().unwrap_or(&[]);
            let line = trim(line);
            if line.is_empty() {
                continue;
            }
            let Some(colon) = line.iter().position(|byte| *byte == b':') else {
                return Self::defaults();
            };
            let name = trim(line.get(..colon).unwrap_or(&[]));
            let Some(database) = database(name) else {
                continue;
            };
            let mut list = SourceList::new();
            for token in trim(line.get(colon.saturating_add(1)..).unwrap_or(&[]))
                .split(|byte| byte.is_ascii_whitespace())
                .filter(|token| !token.is_empty())
            {
                if token.first() == Some(&b'[') {
                    if is_notfound_return(token) {
                        list.set_last_notfound();
                    }
                    continue;
                }
                list.push(SourceEntry {
                    source: source(token),
                    on_notfound_return: false,
                });
            }
            match database {
                Database::Hosts => value.hosts = list,
                Database::Passwd => value.passwd = list,
                Database::Group => value.group = list,
                Database::Services => value.services = list,
                Database::Protocols => value.protocols = list,
                Database::Networks => value.networks = list,
            }
        }
        value
    }

    /// The list for one database.
    #[must_use]
    pub fn sources(&self, db: Database) -> SourceList {
        match db {
            Database::Hosts => self.hosts,
            Database::Passwd => self.passwd,
            Database::Group => self.group,
            Database::Services => self.services,
            Database::Protocols => self.protocols,
            Database::Networks => self.networks,
        }
    }
}

impl Default for Nsswitch {
    fn default() -> Self {
        Self::defaults()
    }
}

/// Load `/etc/nsswitch.conf`; an unreadable or malformed file gives the defaults.
#[must_use]
pub fn load(fs: &dyn FileSource) -> Nsswitch {
    match fs.read_file("/etc/nsswitch.conf", 1024 * 1024) {
        Ok(bytes) => Nsswitch::parse(&bytes),
        Err(_) => Nsswitch::defaults(),
    }
}

/// Run the sources until one returns `Success` or a `[NOTFOUND=return]` stops the search.
pub fn dispatch<F: FnMut(Source) -> Status>(list: &SourceList, mut f: F) -> Status {
    for entry in list.iter() {
        if entry.source == Source::Skipped {
            continue;
        }
        let status = f(entry.source);
        match status {
            Status::Success => return Status::Success,
            Status::NotFound if entry.on_notfound_return => return Status::NotFound,
            _ => {}
        }
    }
    Status::Unavail
}

/// A bounded line scanner that skips an over-long line whole.
pub struct ColonScanner<'a> {
    data: &'a [u8],
    pos: usize,
    max_line: usize,
}

impl<'a> ColonScanner<'a> {
    /// Create a scanner over `data`.
    #[must_use]
    pub const fn new(data: &'a [u8], max_line: usize) -> Self {
        Self {
            data,
            pos: 0,
            max_line,
        }
    }

    /// Return the next line without its newline, or `None`.
    pub fn next_line(&mut self) -> Option<&'a [u8]> {
        while self.pos < self.data.len() {
            let rest = self.data.get(self.pos..)?;
            let end = rest.iter().position(|byte| *byte == b'\n');
            let (line, next) = match end {
                Some(index) => (
                    rest.get(..index)?,
                    self.pos.checked_add(index)?.checked_add(1)?,
                ),
                None => (rest, self.data.len()),
            };
            self.pos = next;
            if line.len() > self.max_line {
                continue;
            }
            return Some(trim_cr(line));
        }
        None
    }

    /// Split one line on `:` into `out`; return the field count.
    pub fn split_fields<'b>(line: &'b [u8], out: &mut [&'b [u8]]) -> usize {
        let mut count = 0usize;
        for field in line.split(|byte| *byte == b':') {
            let Some(slot) = out.get_mut(count) else {
                break;
            };
            *slot = field;
            count = count.saturating_add(1);
        }
        count
    }
}

fn list(entries: &[SourceEntry]) -> SourceList {
    let mut list = SourceList::new();
    for entry in entries {
        list.push(*entry);
    }
    list
}

fn files_only() -> SourceList {
    list(&[SourceEntry {
        source: Source::Files,
        on_notfound_return: false,
    }])
}

fn trim(bytes: &[u8]) -> &[u8] {
    let mut start = 0usize;
    while start < bytes.len()
        && bytes
            .get(start)
            .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        start = start.saturating_add(1);
    }
    let mut end = bytes.len();
    while end > start
        && bytes
            .get(end.saturating_sub(1))
            .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        end = end.saturating_sub(1);
    }
    bytes.get(start..end).unwrap_or(&[])
}

fn trim_cr(bytes: &[u8]) -> &[u8] {
    match bytes.last() {
        Some(b'\r') => bytes.get(..bytes.len().saturating_sub(1)).unwrap_or(bytes),
        _ => bytes,
    }
}

fn database(name: &[u8]) -> Option<Database> {
    match name {
        b"hosts" => Some(Database::Hosts),
        b"passwd" => Some(Database::Passwd),
        b"group" => Some(Database::Group),
        b"services" => Some(Database::Services),
        b"protocols" => Some(Database::Protocols),
        b"networks" => Some(Database::Networks),
        _ => None,
    }
}

fn source(name: &[u8]) -> Source {
    match name {
        b"files" => Source::Files,
        b"dns" => Source::Dns,
        _ => Source::Skipped,
    }
}

fn is_notfound_return(token: &[u8]) -> bool {
    let token = token
        .strip_prefix(b"[")
        .and_then(|value| value.strip_suffix(b"]"))
        .unwrap_or(token);
    let mut parts = token.split(|byte| *byte == b'=');
    let status = parts.next().unwrap_or(&[]);
    let action = parts.next().unwrap_or(&[]);
    status.eq_ignore_ascii_case(b"NOTFOUND") && action.eq_ignore_ascii_case(b"return")
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
