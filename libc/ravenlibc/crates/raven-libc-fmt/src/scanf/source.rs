// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/internal/shgetc.c and musl:src/internal/shgetc.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// The donor files have no per-file header; they are covered by musl/COPYRIGHT (lines 1-24;
// the permission paragraph runs from "Permission is hereby granted" to "DEALINGS IN THE
// SOFTWARE."). MIT via musl/COPYRIGHT.
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//
//! The width-limited scan cursor (`shlim`/`shgetc`/`shunget`/`shcnt`).
//!
//! The donor's `FILE` fields collapse into [`Limited`]: a byte limit so `%5d` never reads
//! the sixth character, a per-directive consumed counter used by `%n`, and a small
//! pushback stack for the look-ahead of the float lexer and the one-character unget of the
//! engine.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The cursor arithmetic is the donor's; every index is bounded by the fixed pushback
// array and the caller's limit.

use crate::traits::CharUnit;

/// The pushback depth of the scan cursor.
///
/// musl's `FILE` reserves `UNGET = 8` bytes; the float lexer reads ahead at most the length
/// of `infinity` plus one or a short `nan(...)` payload, so 32 is ample for the engine and
/// keeps the wide cursor small.
pub(crate) const UNGET: usize = 32;

/// A scan input with a pushback stack.
pub trait ScanSource<C: CharUnit> {
    /// The next input unit; `None` is end of input or an error (the glue knows which).
    fn getc(&mut self) -> Option<C>;

    /// Push one unit back; at least one unit of pushback is guaranteed.
    fn ungetc(&mut self, c: C);
}

/// A width-limited cursor over a [`ScanSource`].
pub struct Limited<'a, S: ScanSource<C>, C: CharUnit> {
    src: &'a mut S,
    limit: usize,
    count: usize,
    push: [C; UNGET],
    plen: usize,
    no_unget: bool,
}

impl<'a, S: ScanSource<C>, C: CharUnit> Limited<'a, S, C> {
    /// A cursor with no limit.
    pub fn new(src: &'a mut S) -> Self {
        Self {
            src,
            limit: 0,
            count: 0,
            push: [C::default(); UNGET],
            plen: 0,
            no_unget: false,
        }
    }

    /// The donor's `shlim`: `limit == 0` means unlimited; the per-directive count restarts.
    pub fn shlim(&mut self, limit: usize) {
        self.limit = limit;
        self.count = 0;
        self.no_unget = false;
    }

    /// The donor's `shcnt`: units consumed since the last [`Limited::shlim`].
    #[must_use]
    pub fn count(&self) -> usize {
        self.count
    }

    /// The donor's `shgetc`; `None` at the limit or at end of input.
    pub fn getc(&mut self) -> Option<C> {
        if self.plen > 0 {
            self.plen = self.plen.saturating_sub(1);
            let c = self.push.get(self.plen).copied().unwrap_or_default();
            self.count = self.count.saturating_add(1);
            return Some(c);
        }
        if self.limit != 0 && self.count >= self.limit {
            self.limit = 0;
            self.no_unget = true;
            return None;
        }
        let Some(c) = self.src.getc() else {
            self.no_unget = true;
            return None;
        };
        self.count = self.count.saturating_add(1);
        Some(c)
    }

    /// The donor's `shunget`; one unit of pushback.
    pub fn ungetc(&mut self, c: C) {
        if self.no_unget {
            return;
        }
        if self.plen < self.push.len() {
            if let Some(slot) = self.push.get_mut(self.plen) {
                *slot = c;
            }
            self.plen = self.plen.saturating_add(1);
            self.count = self.count.saturating_sub(1);
        }
    }

    /// Push every stored unit back into the source, in reverse read order.
    ///
    /// The engine calls this once at the end so the caller's file position is correct.
    pub fn finish(&mut self) {
        while self.plen > 0 {
            self.plen = self.plen.saturating_sub(1);
            let c = self.push.get(self.plen).copied().unwrap_or_default();
            self.src.ungetc(c);
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "source_ut.rs"]
mod tests;
