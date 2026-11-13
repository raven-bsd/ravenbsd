// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2002 Daniel Hartmeier
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/atexit.c and atexit.h
// @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: atexit.c,v 1.29 2022/12/27 17:10:06 jmc Exp $)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
//
//    - Redistributions of source code must retain the above copyright
//      notice, this list of conditions and the following disclaimer.
//    - Redistributions in binary form must reproduce the above
//      copyright notice, this list of conditions and the following
//      disclaimer in the documentation and/or other materials provided
//      with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
// FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
// COPYRIGHT HOLDERS OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
// INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
// BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
// LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN
// ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.

#![allow(clippy::collapsible_if)] // RavenBSD: the donor scans slots in two nested steps

//! The exit-handler table.
//!
//! The first 32 registrations live inline and never allocate (A-stdlib-10); later ones go to
//! the spill `Vec`, and a failed spill is `ENOMEM`. `next_to_run` marks the entry consumed
//! **before** returning it (the donor's `fn_ptr = NULL`) so a handler that calls `exit` cannot
//! run twice. The `mmap`/`mprotect` read-only pages of the donor are not reproduced (RD-new 2).

use alloc::vec::Vec;

use raven_libc_abi::errno::{ENOMEM, Errno};

/// The kind of registration.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Kind {
    /// `atexit(fn)`.
    Atexit,
    /// `__cxa_atexit(fn, arg, dso)`.
    Cxa,
}

/// One exit handler; all addresses are opaque words.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct Entry {
    /// The function address.
    pub func: usize,
    /// The `__cxa_atexit` argument.
    pub arg: usize,
    /// The DSO handle, or 0 for a plain `atexit`.
    pub dso: usize,
    /// The registration kind.
    pub kind: Kind,
}

const EMPTY: Entry = Entry {
    func: 0,
    arg: 0,
    dso: 0,
    kind: Kind::Atexit,
};

const INLINE: usize = 32;

/// The global `atexit`/`__cxa_atexit` table.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct ExitTable {
    inline: [Entry; INLINE],
    inline_len: usize,
    spill: Vec<Entry>,
}

impl ExitTable {
    /// An empty table; every page is reserved inline.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            inline: [EMPTY; INLINE],
            inline_len: 0,
            spill: Vec::new(),
        }
    }

    /// Register a handler (newest); `ENOMEM` only from a failed spill allocation.
    ///
    /// # Errors
    ///
    /// [`ENOMEM`] when the 33rd or later entry cannot be appended.
    pub fn register(&mut self, e: Entry) -> Result<(), Errno> {
        if self.inline_len < INLINE {
            if let Some(slot) = self.inline.get_mut(self.inline_len) {
                *slot = e;
                self.inline_len = self.inline_len.wrapping_add(1);
            }
            return Ok(());
        }
        self.spill.try_reserve(1).map_err(|_| errno(ENOMEM))?;
        self.spill.push(e);
        Ok(())
    }

    /// Register at the oldest position: the stdio cleanup runs last.
    ///
    /// # Errors
    ///
    /// [`ENOMEM`] when the spill cannot take the displaced inline entry.
    pub fn register_oldest(&mut self, e: Entry) -> Result<(), Errno> {
        if self.inline_len < INLINE {
            let mut index = self.inline_len;
            while index > 0 {
                if let (Some(source), Some(target)) = (
                    self.inline.get(index.wrapping_sub(1)).copied(),
                    self.inline.get_mut(index),
                ) {
                    *target = source;
                }
                index = index.wrapping_sub(1);
            }
            if let Some(slot) = self.inline.first_mut() {
                *slot = e;
            }
            self.inline_len = self.inline_len.wrapping_add(1);
            return Ok(());
        }
        self.spill.try_reserve(1).map_err(|_| errno(ENOMEM))?;
        let displaced = self
            .inline
            .get(INLINE.wrapping_sub(1))
            .copied()
            .unwrap_or(EMPTY);
        let mut index = INLINE.wrapping_sub(1);
        while index > 0 {
            if let (Some(source), Some(target)) = (
                self.inline.get(index.wrapping_sub(1)).copied(),
                self.inline.get_mut(index),
            ) {
                *target = source;
            }
            index = index.wrapping_sub(1);
        }
        if let Some(slot) = self.inline.first_mut() {
            *slot = e;
        }
        self.spill.insert(0, displaced);
        Ok(())
    }

    /// The newest unconsumed entry, marked consumed before it is returned.
    pub fn next_to_run(&mut self) -> Option<Entry> {
        let mut index = self.spill.len();
        while index > 0 {
            index = index.wrapping_sub(1);
            if let Some(slot) = self.spill.get_mut(index) {
                if slot.func != 0 {
                    let entry = *slot;
                    slot.func = 0;
                    return Some(entry);
                }
            }
        }
        let mut index = self.inline_len;
        while index > 0 {
            index = index.wrapping_sub(1);
            if let Some(slot) = self.inline.get_mut(index) {
                if slot.func != 0 {
                    let entry = *slot;
                    slot.func = 0;
                    return Some(entry);
                }
            }
        }
        None
    }

    /// The next unconsumed entry registered by `dso` (`dlclose`); `dso == 0` never matches.
    pub fn finalize_dso(&mut self, dso: usize) -> Option<Entry> {
        if dso == 0 {
            return None;
        }
        let mut index = self.spill.len();
        while index > 0 {
            index = index.wrapping_sub(1);
            if let Some(slot) = self.spill.get_mut(index) {
                if slot.func != 0 && slot.dso == dso {
                    let entry = *slot;
                    slot.func = 0;
                    return Some(entry);
                }
            }
        }
        let mut index = self.inline_len;
        while index > 0 {
            index = index.wrapping_sub(1);
            if let Some(slot) = self.inline.get_mut(index) {
                if slot.func != 0 && slot.dso == dso {
                    let entry = *slot;
                    slot.func = 0;
                    return Some(entry);
                }
            }
        }
        None
    }

    /// The next unconsumed entry whose `func` lies in one of the `(base, len)` ranges.
    pub fn finalize_range(&mut self, ranges: &[(usize, usize)]) -> Option<Entry> {
        self.finalize_match(&mut |func| {
            ranges
                .iter()
                .any(|(base, length)| func >= *base && func.wrapping_sub(*base) < *length)
        })
    }

    fn finalize_match(&mut self, matches: &mut dyn FnMut(usize) -> bool) -> Option<Entry> {
        let mut index = self.spill.len();
        while index > 0 {
            index = index.wrapping_sub(1);
            if let Some(slot) = self.spill.get_mut(index) {
                if slot.func != 0 && matches(slot.func) {
                    let entry = *slot;
                    slot.func = 0;
                    return Some(entry);
                }
            }
        }
        let mut index = self.inline_len;
        while index > 0 {
            index = index.wrapping_sub(1);
            if let Some(slot) = self.inline.get_mut(index) {
                if slot.func != 0 && matches(slot.func) {
                    let entry = *slot;
                    slot.func = 0;
                    return Some(entry);
                }
            }
        }
        None
    }

    /// The number of unconsumed entries (`sysconf(_SC_ATEXIT_MAX)` is -1; RD-26).
    #[must_use]
    pub fn pending(&self) -> usize {
        let mut count = 0usize;
        for entry in &self.inline {
            if entry.func != 0 {
                count = count.wrapping_add(1);
            }
        }
        for entry in &self.spill {
            if entry.func != 0 {
                count = count.wrapping_add(1);
            }
        }
        count
    }

    /// The inline capacity (32, A-stdlib-10).
    #[must_use]
    pub const fn inline_capacity(&self) -> usize {
        INLINE
    }
}

impl Default for ExitTable {
    fn default() -> Self {
        Self::new()
    }
}

fn errno(value: i32) -> Errno {
    Errno(value as u16)
}

#[cfg(test)]
#[path = "atexit_ut.rs"]
mod tests;
