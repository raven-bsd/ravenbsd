// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990, 1993
//	The Regents of the University of California.  All rights reserved.
//
// This code is derived from software contributed to Berkeley by
// Chris Torek.
//
// Copyright (c) 2011 The FreeBSD Foundation
// Portions of this software were developed by David Chisnall
// under sponsorship from the FreeBSD Foundation.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/stdio/printfcommon.h @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
//! The `'` thousands-grouping iterator (`grouping_init`/`grouping_print`).
//!
//! The donor walks the locale's `grouping` byte string: the first group may be shorter,
//! later groups repeat the last length. The engine writes the separator through the sink;
//! in wide output the separator is decoded from multibyte once per group.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The group lengths come from the locale string and are bounded by the digit buffer; the
// loop has an explicit guard, so no index can run away.

use raven_libc_ctype::{MbMode, MbState, Mbr, mbrtowc};

use crate::traits::{CharUnit, Sink, SinkError};
use crate::types::{FmtErr, NumericFormat};

/// The donor's `CHAR_MAX` sentinel for the grouping string.
const CHAR_MAX: u8 = 0x7f;

/// The grouping state of one conversion.
pub(crate) struct Grouping<'a> {
    sep: &'a [u8],
    grouping: &'a [u8],
    lead: i32,
    nseps: i32,
    nrepeats: i32,
    gidx: usize,
}

impl<'a> Grouping<'a> {
    /// The donor's `grouping_init`; returns the extra output units the separators need.
    pub(crate) fn new(ndigits: i32, nf: &'a NumericFormat<'a>, wide: bool) -> (Self, usize) {
        let mut gs = Self {
            sep: nf.thousands_sep,
            grouping: nf.grouping,
            lead: ndigits,
            nseps: 0,
            nrepeats: 0,
            gidx: 0,
        };
        let mut guard = 0u32;
        loop {
            guard = guard.wrapping_add(1);
            let cur = gs.grouping.get(gs.gidx).copied().unwrap_or(CHAR_MAX);
            // The C locale has an empty (or NUL) grouping string: no separators.
            if cur == CHAR_MAX || cur == 0 {
                break;
            }
            if gs.lead <= i32::from(cur) {
                break;
            }
            gs.lead = gs.lead.wrapping_sub(i32::from(cur));
            let next = gs
                .grouping
                .get(gs.gidx.wrapping_add(1))
                .copied()
                .unwrap_or(0);
            if next != 0 {
                gs.nseps = gs.nseps.wrapping_add(1);
                gs.gidx = gs.gidx.wrapping_add(1);
            } else {
                gs.nrepeats = gs.nrepeats.wrapping_add(1);
            }
            if guard > 128 {
                // A hostile grouping string cannot make the engine loop.
                break;
            }
        }
        let count = gs.nseps.wrapping_add(gs.nrepeats) as usize;
        let per = if gs.sep.is_empty() {
            0
        } else if wide {
            1
        } else {
            gs.sep.len()
        };
        let extra = count.saturating_mul(per);
        (gs, extra)
    }

    /// Whether any separator will be written (test only).
    #[cfg(test)]
    #[must_use]
    pub(crate) fn active(&self) -> bool {
        self.nseps > 0 || self.nrepeats > 0
    }

    /// Print `digits[0..end]` with separators; returns the number of digit characters used.
    ///
    /// # Errors
    /// [`FmtErr::Sink`] when the sink fails.
    pub(crate) fn print<C: CharUnit, S: Sink<C>>(
        mut self,
        sink: &mut S,
        digits: &[u8],
        end: usize,
        mode: MbMode,
    ) -> Result<usize, FmtErr> {
        let mut off = 0usize;
        piece(sink, digits, &mut off, self.lead, end)?;
        let mut guard = 0u32;
        while self.nseps > 0 || self.nrepeats > 0 {
            guard = guard.wrapping_add(1);
            if guard > 128 {
                break;
            }
            if self.nrepeats > 0 {
                self.nrepeats = self.nrepeats.wrapping_sub(1);
            } else {
                self.gidx = self.gidx.saturating_sub(1);
                self.nseps = self.nseps.wrapping_sub(1);
            }
            write_sep(sink, self.sep, mode)?;
            let len = i32::from(self.grouping.get(self.gidx).copied().unwrap_or(0));
            piece(sink, digits, &mut off, len, end)?;
        }
        Ok(if off > end { end } else { off })
    }
}

fn piece<C: CharUnit, S: Sink<C>>(
    sink: &mut S,
    digits: &[u8],
    off: &mut usize,
    len: i32,
    end: usize,
) -> Result<(), FmtErr> {
    if len <= 0 {
        return Ok(());
    }
    let want = len as usize;
    let avail = end.saturating_sub(*off);
    let take = if avail < want { avail } else { want };
    if let Some(bytes) = digits.get(*off..off.saturating_add(take)) {
        write_ascii(sink, bytes)?;
    }
    if take < want {
        sink.pad(C::from_ascii(b'0'), want.saturating_sub(take))
            .map_err(|_| FmtErr::Sink)?;
    }
    *off = off.saturating_add(want);
    Ok(())
}

/// Write ASCII bytes as output units (narrow: one unit per byte).
pub(crate) fn write_ascii<C: CharUnit, S: Sink<C>>(
    sink: &mut S,
    bytes: &[u8],
) -> Result<(), FmtErr> {
    for &b in bytes {
        sink.write(&[C::from_ascii(b)]).map_err(|_| FmtErr::Sink)?;
    }
    Ok(())
}

/// Write one multibyte string as output units (wide output decodes it).
pub(crate) fn write_mb<C: CharUnit, S: Sink<C>>(
    sink: &mut S,
    bytes: &[u8],
    mode: MbMode,
) -> Result<(), FmtErr> {
    if !C::WIDE {
        return write_ascii(sink, bytes);
    }
    let mut state = MbState::INITIAL;
    let mut i = 0usize;
    while i < bytes.len() {
        match mbrtowc(&mut state, mode, bytes.get(i..).unwrap_or(&[])) {
            Mbr::Char { wc, used } => {
                sink.write(&[C::from_u32(wc)]).map_err(|_| FmtErr::Sink)?;
                i = i.saturating_add(used.max(1));
            }
            Mbr::Incomplete | Mbr::Ilseq => return Err(FmtErr::Encoding),
        }
    }
    Ok(())
}

fn write_sep<C: CharUnit, S: Sink<C>>(
    sink: &mut S,
    sep: &[u8],
    mode: MbMode,
) -> Result<(), FmtErr> {
    if sep.is_empty() {
        return Ok(());
    }
    let _: Result<(), SinkError> = Ok(());
    write_mb(sink, sep, mode)
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
#[path = "group_ut.rs"]
mod tests;
