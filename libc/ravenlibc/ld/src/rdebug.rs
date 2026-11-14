// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright 1996, 1997, 1998, 1999, 2000 John D. Polstra.
// Copyright 2003 Alexander Kabaev <kan@FreeBSD.ORG>.
// Copyright 2009-2013 Konstantin Belousov <kib@FreeBSD.ORG>.
// Copyright 2012 John Marino <draco@marino.st>.
// Copyright 2014-2017 The FreeBSD Foundation
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @
// 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: 133344405f532bd54ff3629a77d4c4e062f01fdb)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! The `r_debug` protocol: the object list a debugger reads.

use raven_libc_elf::ObjId;

/// The maximum number of link-map entries.
pub const MAX_MAP: usize = 64;

/// One `link_map` entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkMap {
    /// The base address.
    pub l_addr: u64,
    /// The name address (0 until the shell materialises it).
    pub l_name: u64,
    /// The dynamic section address.
    pub l_ld: u64,
    /// The next object.
    pub l_next: u32,
    /// The previous object.
    pub l_prev: u32,
    /// The loader object id.
    pub id: u32,
}

/// The `r_debug` record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RDebug {
    /// `r_version`.
    pub version: i32,
    /// Head of the link map.
    pub head: u32,
    /// `r_brk`: the debugger breakpoint.
    pub brk: u64,
    /// `RT_ADD`/`RT_DELETE`/`RT_CONSISTENT`.
    pub state: i32,
    /// `r_ldbase`.
    pub ldbase: u64,
}

impl RDebug {
    /// A consistent, empty record.
    #[must_use]
    pub const fn new(ldbase: u64) -> Self {
        Self {
            version: 1,
            head: u32::MAX,
            brk: 0,
            state: 0,
            ldbase,
        }
    }
}

/// The link-map table.
pub struct LinkMapTable {
    entries: [LinkMap; MAX_MAP],
    used: [bool; MAX_MAP],
}

impl LinkMapTable {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: [LinkMap {
                l_addr: 0,
                l_name: 0,
                l_ld: 0,
                l_next: u32::MAX,
                l_prev: u32::MAX,
                id: 0,
            }; MAX_MAP],
            used: [false; MAX_MAP],
        }
    }

    /// Add an object; returns its link-map index.
    pub fn add(&mut self, id: ObjId, addr: u64, ld: u64) -> Option<u32> {
        for index in 0..MAX_MAP {
            if self.used.get(index).copied() == Some(false) {
                let slot = self.entries.get_mut(index)?;
                slot.l_addr = addr;
                slot.l_ld = ld;
                slot.id = id.0;
                slot.l_next = u32::MAX;
                slot.l_prev = u32::MAX;
                if let Some(used) = self.used.get_mut(index) {
                    *used = true;
                }
                return u32::try_from(index).ok();
            }
        }
        None
    }

    /// Remove an object.
    pub fn delete(&mut self, index: u32) -> bool {
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        if let Some(used) = self.used.get_mut(index) {
            *used = false;
            true
        } else {
            false
        }
    }

    /// The entry at `index`.
    #[must_use]
    pub fn get(&self, index: u32) -> Option<&LinkMap> {
        let index = usize::try_from(index).ok()?;
        if self.used.get(index).copied() == Some(true) {
            self.entries.get(index)
        } else {
            None
        }
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.used.iter().filter(|used| **used).count()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for LinkMapTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "rdebug_ut.rs"]
mod tests;
