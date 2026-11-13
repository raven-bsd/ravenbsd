// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1992, 1993 The Regents of the University of California.  All rights reserved.
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/getcap.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: getcap.c,v 1.44 2016/01/08 18:57:57 tb Exp $)
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

//! `getcap` (donor-map 15 T3, 02 B14, delta D-6).
//!
//! The text-file path only: every `db.h`/`.db`/`cdbget`/`cgetusedb` path is deleted (R19,
//! Berkeley DB is ring 3). Files are read whole through [`ReadFile`] and parsed in memory, so
//! the donor's `FILE` recursion and its `toprec`/`fseeko` cursor become a record cache in
//! [`CapDb`]. The `tc=` recursion limit is the donor's `MAX_RECURSION 32` (a chain of 32 levels
//! is accepted, the 33rd is [`CgetError::Loop`]); a record cap of 1 MiB is enforced by
//! [`ReadFile`]. An unresolvable `tc=` leaves the `tc=` text in the record and returns `Ok`,
//! like the donor's `return (1)`.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // direct transliteration: the indices are bounded by the donor algorithm
use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;

use crate::seams::ReadFile;

/// The donor's `BFRAG` record growth chunk.
pub const BFRAG: usize = 1024;
/// The donor's `SFRAG` string growth chunk.
pub const SFRAG: usize = 100;
/// The donor's `ESC`.
pub const ESC: u8 = 0x1b;
/// `MAX_RECURSION`: the deepest `tc=` chain accepted (the donor errors at `depth > 32`).
pub const MAX_RECURSION: usize = 32;
/// The cap of one record (ticket 4.1): 1 MiB.
pub const RECORD_CAP: usize = 1024 * 1024;

/// A `getcap` error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CgetError {
    /// The record was not found (`-1`).
    NotFound,
    /// A `tc=` loop or the recursion limit (`-3`/`-4`).
    Loop,
    /// Out of memory (`-2`).
    Nomem,
    /// A malformed record or capability.
    Syntax,
    /// The underlying `ReadFile` error.
    Errno(Errno),
}

fn errno_of(value: i32) -> Errno {
    Errno(value as u16)
}

/// `cgetmatch`: whether `name` is one of the record's names.
#[must_use]
pub fn matches(buf: &[u8], name: &[u8]) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut bp = 0usize;
    loop {
        let mut np = 0usize;
        loop {
            if np == name.len() {
                let at = buf.get(bp).copied().unwrap_or(0);
                if at == b'|' || at == b':' || at == 0 {
                    return true;
                }
                break;
            }
            if buf.get(bp).copied().unwrap_or(0) != name[np] {
                break;
            }
            bp = bp.wrapping_add(1);
            np = np.wrapping_add(1);
        }
        loop {
            let at = buf.get(bp).copied().unwrap_or(0);
            if at == 0 || at == b':' {
                return false;
            }
            bp = bp.wrapping_add(1);
            if at == b'|' {
                break;
            }
        }
    }
}

/// `cgetcap`: the offset of the value of `cap`, after the `:`/`=` (`kind`) character.
#[must_use]
pub fn cap(buf: &[u8], cap: &[u8], kind: u8) -> Option<usize> {
    cap_at(buf, cap, kind, 0)
}

fn cap_at(buf: &[u8], cap: &[u8], kind: u8, start: usize) -> Option<usize> {
    let mut bp = start;
    loop {
        loop {
            match buf.get(bp).copied() {
                None => return None,
                Some(b':') => {
                    bp = bp.wrapping_add(1);
                    break;
                }
                Some(_) => bp = bp.wrapping_add(1),
            }
        }
        let mut cp = 0usize;
        while cp < cap.len() && buf.get(bp).copied() == Some(cap[cp]) {
            cp = cp.wrapping_add(1);
            bp = bp.wrapping_add(1);
        }
        if cp != cap.len() {
            continue;
        }
        if buf.get(bp).copied() == Some(b'@') {
            return None;
        }
        if kind == b':' {
            match buf.get(bp).copied() {
                None | Some(b':') => return Some(bp),
                _ => continue,
            }
        }
        if buf.get(bp).copied() != Some(kind) {
            continue;
        }
        bp = bp.wrapping_add(1);
        if buf.get(bp).copied() == Some(b'@') {
            return None;
        }
        return Some(bp);
    }
}

/// `cgetnum`: a decimal, hexadecimal (`0x`) or octal (`0`) numeric capability.
pub fn num(buf: &[u8], cap_name: &[u8]) -> Result<i64, CgetError> {
    let start = cap(buf, cap_name, b'#').ok_or(CgetError::NotFound)?;
    let mut bp = start;
    let mut value: i64 = 0;
    let base: i64 = match buf.get(bp).copied() {
        Some(b'0') => {
            bp = bp.wrapping_add(1);
            match buf.get(bp).copied() {
                Some(b'x') | Some(b'X') => {
                    bp = bp.wrapping_add(1);
                    16
                }
                _ => 8,
            }
        }
        _ => 10,
    };
    loop {
        let digit = match buf.get(bp).copied() {
            Some(c @ b'0'..=b'9') => i64::from(c.wrapping_sub(b'0')),
            Some(c @ b'a'..=b'f') => i64::from(c.wrapping_sub(b'a')).wrapping_add(10),
            Some(c @ b'A'..=b'F') => i64::from(c.wrapping_sub(b'A')).wrapping_add(10),
            _ => break,
        };
        if digit >= base {
            break;
        }
        value = value.wrapping_mul(base).wrapping_add(digit);
        bp = bp.wrapping_add(1);
    }
    Ok(value)
}

fn value_end(buf: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < buf.len() && buf[end] != b':' {
        end = end.wrapping_add(1);
    }
    end
}

/// `cgetstr`: a string capability with `^`/`\` escapes decoded.
pub fn str_(buf: &[u8], cap_name: &[u8]) -> Result<Vec<u8>, CgetError> {
    let start = cap(buf, cap_name, b'=').ok_or(CgetError::NotFound)?;
    let end = value_end(buf, start);
    let mut out = Vec::new();
    let mut bp = start;
    while bp < end {
        match buf[bp] {
            b'^' => {
                bp = bp.wrapping_add(1);
                if bp >= end {
                    break;
                }
                out.push(buf[bp] & 0o037);
                bp = bp.wrapping_add(1);
            }
            b'\\' => {
                bp = bp.wrapping_add(1);
                if bp >= end {
                    break;
                }
                if (b'0'..=b'7').contains(&buf[bp]) {
                    let mut n: u8 = 0;
                    let mut count: u8 = 3;
                    while count > 0 && bp < end && (b'0'..=b'7').contains(&buf[bp]) {
                        n = n.wrapping_mul(8).wrapping_add(buf[bp].wrapping_sub(b'0'));
                        bp = bp.wrapping_add(1);
                        count = count.wrapping_sub(1);
                    }
                    out.push(n);
                } else {
                    let c = buf[bp];
                    bp = bp.wrapping_add(1);
                    out.push(match c {
                        b'b' | b'B' => 0x08,
                        b't' | b'T' => b'\t',
                        b'n' | b'N' => b'\n',
                        b'f' | b'F' => 0x0c,
                        b'r' | b'R' => b'\r',
                        b'e' | b'E' => ESC,
                        b'c' | b'C' => b':',
                        _ => c,
                    });
                }
            }
            c => {
                out.push(c);
                bp = bp.wrapping_add(1);
            }
        }
    }
    Ok(out)
}

/// `cgetustr`: a string capability with the escapes kept.
pub fn ustr(buf: &[u8], cap_name: &[u8]) -> Result<Vec<u8>, CgetError> {
    let start = cap(buf, cap_name, b'=').ok_or(CgetError::NotFound)?;
    let end = value_end(buf, start);
    Ok(buf[start..end].to_vec())
}

/// Split a text file into cap records, implementing the donor's `\`-continuation rule.
fn records(text: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut record = Vec::new();
    let mut start = 0usize;
    while start < text.len() {
        let rest = text.get(start..).unwrap_or(b"");
        let (line, next) = match rest.iter().position(|&c| c == b'\n') {
            Some(position) => (
                rest.get(..position).unwrap_or(b""),
                start.wrapping_add(position).wrapping_add(1),
            ),
            None => (rest, text.len()),
        };
        start = next;
        let continued = line.last().copied() == Some(b'\\');
        let body = if continued {
            line.get(..line.len().wrapping_sub(1)).unwrap_or(b"")
        } else {
            line
        };
        record.extend_from_slice(body);
        if continued && start < text.len() {
            continue;
        }
        if record.len() > RECORD_CAP {
            return out;
        }
        out.push(core::mem::take(&mut record));
    }
    if !record.is_empty() {
        out.push(record);
    }
    out
}

/// A capability database with the donor's `cgetset` top record and the `cgetnext` cursor.
#[derive(Clone, Debug, Default)]
pub struct CapDb {
    toprec: Option<Vec<u8>>,
    gottoprec: bool,
    cache: Option<Vec<Vec<u8>>>,
    cursor: usize,
}

fn read_records<R: ReadFile>(r: &mut R, files: &[&[u8]]) -> (Vec<Vec<u8>>, Option<Errno>) {
    let mut all = Vec::new();
    let mut last_error = None;
    let mut any = false;
    for file in files {
        let mut contents = Vec::new();
        match r.read_all(file, &mut contents) {
            Ok(()) => {
                any = true;
                all.extend(records(&contents));
            }
            Err(error) => last_error = Some(error),
        }
    }
    (if any { all } else { Vec::new() }, last_error)
}

impl CapDb {
    /// `cgetset`-compatible default state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `cgetset`: push (or clear) a top record.
    pub fn set(&mut self, ent: Option<&[u8]>) -> Result<(), Errno> {
        match ent {
            Some(value) => {
                if value.len() > RECORD_CAP {
                    return Err(errno_of(raven_libc_abi::errno::ENOMEM));
                }
                self.toprec = Some(value.to_vec());
                self.gottoprec = false;
            }
            None => {
                self.toprec = None;
            }
        }
        Ok(())
    }

    /// `cgetent`: find `name`, expand every `tc=` chain.
    pub fn ent<R: ReadFile>(
        &mut self,
        r: &mut R,
        files: &[&[u8]],
        name: &[u8],
    ) -> Result<Vec<u8>, CgetError> {
        let (record, _unresolved) = self.getent(r, files, name, 0)?;
        Ok(record)
    }

    fn getent<R: ReadFile>(
        &mut self,
        r: &mut R,
        files: &[&[u8]],
        name: &[u8],
        depth: usize,
    ) -> Result<(Vec<u8>, bool), CgetError> {
        if depth > MAX_RECURSION {
            return Err(CgetError::Loop);
        }
        let mut record;
        if depth == 0 {
            match self.toprec.as_ref() {
                Some(top) if matches(top, name) => {
                    record = top.clone();
                }
                _ => {
                    let (cache, last_error) = read_records(r, files);
                    if cache.is_empty() && last_error.is_some() {
                        return Err(CgetError::Errno(
                            last_error.unwrap_or(errno_of(raven_libc_abi::errno::ENOENT)),
                        ));
                    }
                    record = cache
                        .into_iter()
                        .filter(|candidate| !candidate.is_empty() && candidate[0] != b'#')
                        .find(|candidate| matches(candidate, name))
                        .ok_or(CgetError::NotFound)?;
                }
            }
        } else {
            let (cache, last_error) = read_records(r, files);
            if cache.is_empty() && last_error.is_some() {
                return Err(CgetError::Errno(
                    last_error.unwrap_or(errno_of(raven_libc_abi::errno::ENOENT)),
                ));
            }
            record = cache
                .into_iter()
                .filter(|candidate| !candidate.is_empty() && candidate[0] != b'#')
                .find(|candidate| matches(candidate, name))
                .ok_or(CgetError::NotFound)?;
        }
        if record.len() > RECORD_CAP {
            return Err(CgetError::Nomem);
        }

        let mut unresolved = false;
        let mut scan = 0usize;
        while let Some(value_start) = cap_at(&record, b"tc", b'=', scan) {
            let mut end = value_start;
            while end < record.len() && record[end] != b':' {
                end = end.wrapping_add(1);
            }
            if end >= record.len() {
                // The donor stomps the trailing ':' (or the NUL) and recurses with the value.
                let tc_value = record[value_start..].to_vec();
                match self.getent(r, files, &tc_value, depth.wrapping_add(1)) {
                    Ok((mut interpolated, _)) => {
                        interpolated = strip_name(&interpolated);
                        let start = value_start.saturating_sub(3);
                        record.splice(start.., interpolated.iter().copied());
                        scan = start;
                        continue;
                    }
                    Err(CgetError::NotFound) => {
                        unresolved = true;
                        scan = end;
                        continue;
                    }
                    Err(error) => return Err(error),
                }
            }
            let tc_value = record[value_start..end].to_vec();
            let tcstart = value_start.saturating_sub(3);
            let tcend = end.wrapping_add(1);
            match self.getent(r, files, &tc_value, depth.wrapping_add(1)) {
                Ok((interpolated, nested_unresolved)) => {
                    unresolved |= nested_unresolved;
                    let mut expanded = strip_name(&interpolated);
                    if expanded.last().copied() != Some(b':') {
                        expanded.push(b':');
                    }
                    record.splice(tcstart..tcend, expanded.iter().copied());
                    scan = tcstart;
                }
                Err(CgetError::NotFound) => {
                    unresolved = true;
                    scan = tcend;
                }
                Err(error) => return Err(error),
            }
        }
        Ok((record, unresolved))
    }

    /// `cgetfirst`: restart the enumeration.
    pub fn first<R: ReadFile>(&mut self, r: &mut R, files: &[&[u8]]) -> Result<Vec<u8>, CgetError> {
        self.close();
        self.next(r, files)
    }

    /// `cgetnext`: the next record, with its `tc=` chains expanded.
    #[allow(clippy::while_let_loop)]
    pub fn next<R: ReadFile>(&mut self, r: &mut R, files: &[&[u8]]) -> Result<Vec<u8>, CgetError> {
        if self.cache.is_none() {
            let (cache, last_error) = read_records(r, files);
            if cache.is_empty() {
                return Err(match last_error {
                    Some(error) => CgetError::Errno(error),
                    None => CgetError::NotFound,
                });
            }
            self.cache = Some(cache);
            self.cursor = 0;
        }
        let record = loop {
            let cache = self.cache.as_ref().ok_or(CgetError::NotFound)?;
            let Some(candidate) = cache.get(self.cursor).cloned() else {
                self.close();
                return Err(CgetError::NotFound);
            };
            self.cursor = self.cursor.wrapping_add(1);
            if !candidate.is_empty() && candidate[0] != b'#' {
                break candidate;
            }
        };
        let name_end = record
            .iter()
            .position(|&c| c == b'|' || c == b':')
            .unwrap_or(record.len());
        let name = record[..name_end].to_vec();
        let saved = self.toprec.clone();
        let saved_got = self.gottoprec;
        self.toprec = Some(record);
        self.gottoprec = true;
        let result = self.getent(r, files, &name, 0);
        self.toprec = saved;
        self.gottoprec = saved_got;
        result.map(|(record, _)| record)
    }

    /// `cgetclose`.
    pub fn close(&mut self) {
        self.toprec = None;
        self.gottoprec = false;
        self.cache = None;
        self.cursor = 0;
    }
}

fn strip_name(record: &[u8]) -> Vec<u8> {
    match record.iter().position(|&c| c == b':') {
        Some(position) => record[position..].to_vec(),
        None => Vec::new(),
    }
}

/// `cgetusedb`: the `.db` path does not exist; the call only records the flag and returns the
/// previous value (the shell keeps the flag).
#[must_use]
pub fn cgetusedb(previous: bool, _new_value: bool) -> bool {
    previous
}

#[cfg(test)]
#[path = "getcap_ut.rs"]
mod tests;
