// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1989, 1993 The Regents of the University of California.  All rights reserved.
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/getttyent.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: getttyent.c,v 1.24 2016/01/08 18:57:57 tb Exp $)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getttyent`/`getttynam`/`setttyent`/`endttyent` (donor-map 15 T3, 02 X82).
//!
//! The whole file is read through [`ReadFile`] and parsed in memory; the donor's `fgets`
//! stream and its static `ttyent` are replaced by [`TtyDb`] and [`TtyEnt`]. The keyword set is
//! the union of the OpenBSD donor (`off on secure local rtscts softcar mdmbuf window=`) and the
//! installed FreeBSD header (`onifexists onifconsole insecure dialup network group= none`), per
//! ticket 5.D finding 2. The OpenBSD-only bits use `TTY_LOCAL 0x40`, `TTY_RTSCTS 0x80`,
//! `TTY_SOFTCAR 0x100`, `TTY_MDMBUF 0x200` (the report asks C38 to add them to `ttyent.h`).
//! `TtyEnt` gains `group` (the FreeBSD `ty_group`) so `group=` keeps its value.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // direct transliteration: the indices are bounded by the donor algorithm
use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;

use crate::seams::ReadFile;

/// `_PATH_TTYS`.
pub const PATH_TTYS: &[u8] = b"/etc/ttys";

/// `TTY_ON`.
pub const TTY_ON: u32 = 0x01;
/// `TTY_SECURE`.
pub const TTY_SECURE: u32 = 0x02;
/// `TTY_DIALUP`.
pub const TTY_DIALUP: u32 = 0x04;
/// `TTY_NETWORK`.
pub const TTY_NETWORK: u32 = 0x08;
/// `TTY_IFEXISTS`.
pub const TTY_IFEXISTS: u32 = 0x10;
/// `TTY_IFCONSOLE`.
pub const TTY_IFCONSOLE: u32 = 0x20;
/// The OpenBSD `local` keyword (new bit, see the module note).
pub const TTY_LOCAL: u32 = 0x40;
/// The OpenBSD `rtscts` keyword (new bit).
pub const TTY_RTSCTS: u32 = 0x80;
/// The OpenBSD `softcar` keyword (new bit).
pub const TTY_SOFTCAR: u32 = 0x100;
/// The OpenBSD `mdmbuf` keyword (new bit).
pub const TTY_MDMBUF: u32 = 0x200;

/// One `/etc/ttys` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TtyEnt {
    /// `ty_name`.
    pub name: Vec<u8>,
    /// `ty_getty`.
    pub getty: Vec<u8>,
    /// `ty_type`.
    pub type_: Vec<u8>,
    /// `ty_status`.
    pub status: u32,
    /// `ty_window` (the value of `window=`).
    pub window: Vec<u8>,
    /// `ty_comment`.
    pub comment: Vec<u8>,
    /// `ty_group` (FreeBSD; the value of `group=`).
    pub group: Vec<u8>,
}

/// The parsed `/etc/ttys` and its cursor.
#[derive(Clone, Debug, Default)]
pub struct TtyDb {
    entries: Vec<TtyEnt>,
    cursor: usize,
}

fn is_space(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' || c == 0x0b || c == 0x0c
}

/// The characters the donor's field splitter treats as separators (`\t`, ` `, `\n`).
fn is_field_space(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b'\n'
}

/// `skip`: split one field, honouring quotes and the `\\"` escape.
///
/// Returns the field, the index after the separator run, and the terminating character
/// (`#` for a comment, a space for a separator, 0 at the end of the line).
fn skip(line: &[u8], mut p: usize) -> (Vec<u8>, usize, u8) {
    let mut field = Vec::new();
    let mut quoted = false;
    while p < line.len() {
        let c = line[p];
        if c == b'"' {
            quoted = !quoted;
            p = p.wrapping_add(1);
            continue;
        }
        if quoted && c == b'\\' && line.get(p.wrapping_add(1)) == Some(&b'"') {
            p = p.wrapping_add(1);
        }
        let c = line[p];
        if !quoted && c == b'#' {
            return (field, p.wrapping_add(1), b'#');
        }
        if !quoted && is_field_space(c) {
            let mut next = p.wrapping_add(1);
            while next < line.len() && is_field_space(line[next]) {
                next = next.wrapping_add(1);
            }
            return (field, next, b' ');
        }
        field.push(c);
        p = p.wrapping_add(1);
    }
    (field, line.len(), 0)
}

fn vcmp<'a>(p: &'a [u8], keyword: &[u8]) -> Option<&'a [u8]> {
    p.strip_prefix(keyword)
        .filter(|_| p.get(keyword.len()) == Some(&b'='))
        .and_then(|value| value.get(1..))
}

fn trim_comment(line: &[u8], start: usize) -> Vec<u8> {
    let mut p = start;
    while p < line.len() && (line[p] == b' ' || line[p] == b'\t') {
        p = p.wrapping_add(1);
    }
    let end = line[p..]
        .iter()
        .position(|&c| c == b'\n')
        .map_or(line.len(), |offset| p.wrapping_add(offset));
    line.get(p..end).unwrap_or(b"").to_vec()
}

fn parse_line(line: &[u8]) -> Option<TtyEnt> {
    let mut p = 0usize;
    while p < line.len() && is_space(line[p]) {
        p = p.wrapping_add(1);
    }
    if p >= line.len() || line[p] == b'#' {
        return None;
    }
    let mut entry = TtyEnt::default();
    let (name, next, terminator) = skip(line, p);
    entry.name = name;
    p = next;
    if terminator == b'#' {
        entry.comment = trim_comment(line, p);
        return Some(entry);
    }
    let (getty, next, terminator) = skip(line, p);
    p = next;
    if !getty.is_empty() {
        entry.getty = getty;
        if terminator != b'#' {
            let (type_, next, _terminator) = skip(line, p);
            p = next;
            if !type_.is_empty() {
                entry.type_ = type_;
            }
        }
    }
    let mut comment = if terminator == b'#' {
        Some(trim_comment(line, p))
    } else {
        None
    };
    while p < line.len() && line[p] != b'\n' {
        let (field, next, terminator) = skip(line, p);
        p = next;
        if terminator == b'#' {
            comment = Some(trim_comment(line, p));
            break;
        }
        if field.is_empty() {
            break;
        }
        if field == b"off" {
            entry.status &= !TTY_ON;
        } else if field == b"on" {
            entry.status |= TTY_ON;
        } else if field == b"secure" {
            entry.status |= TTY_SECURE;
        } else if field == b"insecure" {
            entry.status &= !TTY_SECURE;
        } else if field == b"onifexists" {
            entry.status |= TTY_IFEXISTS;
        } else if field == b"onifconsole" {
            entry.status |= TTY_IFCONSOLE;
        } else if field == b"dialup" {
            entry.status |= TTY_DIALUP;
        } else if field == b"network" {
            entry.status |= TTY_NETWORK;
        } else if field == b"local" {
            entry.status |= TTY_LOCAL;
        } else if field == b"rtscts" {
            entry.status |= TTY_RTSCTS;
        } else if field == b"softcar" {
            entry.status |= TTY_SOFTCAR;
        } else if field == b"mdmbuf" {
            entry.status |= TTY_MDMBUF;
        } else if field == b"none" {
            entry.group = Vec::new();
        } else if let Some(value) = vcmp(&field, b"window") {
            entry.window = value.to_vec();
        } else if let Some(value) = vcmp(&field, b"group") {
            entry.group = value.to_vec();
        } else {
            break;
        }
        if terminator == 0 {
            break;
        }
    }
    if p < line.len() && line[p] == b'#' && comment.is_none() {
        comment = Some(trim_comment(line, p.wrapping_add(1)));
    }
    if let Some(value) = comment {
        entry.comment = value;
    }
    Some(entry)
}

impl TtyDb {
    /// `setttyent`: read and parse `path`.
    pub fn open<R: ReadFile>(r: &mut R, path: &[u8]) -> Result<Self, Errno> {
        let mut contents = Vec::new();
        r.read_all(path, &mut contents)?;
        let mut entries = Vec::new();
        let mut start = 0usize;
        for (index, &byte) in contents.iter().enumerate() {
            if byte == b'\n' {
                if let Some(entry) = parse_line(&contents[start..=index]) {
                    entries.push(entry);
                }
                start = index.wrapping_add(1);
            }
        }
        if start < contents.len()
            && let Some(entry) = parse_line(&contents[start..])
        {
            entries.push(entry);
        }
        Ok(Self { entries, cursor: 0 })
    }

    /// `getttyent`: the next entry.
    #[allow(clippy::should_implement_trait)] // the C name is `getttyent`/`cgetnext`
    pub fn next(&mut self) -> Option<TtyEnt> {
        let entry = self.entries.get(self.cursor).cloned();
        if entry.is_some() {
            self.cursor = self.cursor.wrapping_add(1);
        }
        entry
    }

    /// `getttynam`: the entry whose name equals `tty`.
    #[must_use]
    pub fn by_name(&self, tty: &[u8]) -> Option<TtyEnt> {
        self.entries.iter().find(|entry| entry.name == tty).cloned()
    }

    /// `setttyent`: rewind.
    pub fn rewind(&mut self) {
        self.cursor = 0;
    }

    /// Whether the database holds no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
#[path = "ttyent_ut.rs"]
mod tests;
