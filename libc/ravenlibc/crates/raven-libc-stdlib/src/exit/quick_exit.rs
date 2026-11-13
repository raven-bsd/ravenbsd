// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2011 David Chisnall
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/stdlib/quick_exit.c
// @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: quick_exit.c,v 1.1 2013/04/26 18:29:55 christos Exp $)
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

//! `at_quick_exit`/`quick_exit`: a separate LIFO list, then `_Exit(status)`.

use alloc::vec::Vec;

use raven_libc_abi::errno::{ENOMEM, Errno};

use super::atexit::Entry;

/// The `at_quick_exit` list.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct QuickExitTable {
    entries: Vec<Entry>,
}

impl QuickExitTable {
    /// An empty list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// `at_quick_exit(fn)`; the donor returns 1 on failure, the shell maps `Err` to 1.
    ///
    /// # Errors
    ///
    /// [`ENOMEM`] when the node cannot be allocated.
    pub fn register(&mut self, func: usize) -> Result<(), Errno> {
        self.entries.try_reserve(1).map_err(|_| errno(ENOMEM))?;
        self.entries.push(Entry {
            func,
            arg: 0,
            dso: 0,
            kind: super::atexit::Kind::Atexit,
        });
        Ok(())
    }

    /// The newest handler.
    pub fn pop_next(&mut self) -> Option<Entry> {
        self.entries.pop()
    }

    /// The number of handlers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for QuickExitTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Drain the list LIFO; the shell then calls `_Exit(status)`.
pub fn quick_exit_steps(table: &mut QuickExitTable) -> Vec<Entry> {
    let mut steps = Vec::new();
    while let Some(entry) = table.pop_next() {
        steps.push(entry);
    }
    steps
}

fn errno(value: i32) -> Errno {
    Errno(value as u16)
}

#[cfg(test)]
#[path = "quick_exit_ut.rs"]
mod tests;
