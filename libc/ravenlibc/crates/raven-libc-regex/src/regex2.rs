// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1992, 1993, 1994 Henry Spencer.
// Copyright (c) 1992, 1993, 1994
//	The Regents of the University of California.  All rights reserved.
//
// This code is derived from software contributed to Berkeley by
// Henry Spencer.
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
//	@(#)regex2.h	8.4 (Berkeley) 3/20/94
// $FreeBSD: head/lib/libc/regex/regex2.h 368359 2020-12-05 03:18:48Z kevans $
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/regex/regex2.h @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// Altered: transpiled to Rust; hardening limits MAX_STRIP, BACKREF_BUDGET, MAX_PAREN_DEPTH, BACKREF_DEPTH_MAX; generic state-set engine; wide-character access through RegexLocale.

//! Strip words, the character-set representation and `struct re_guts` (`regex2.h`).
//!
//! A compiled program is a *strip*: `u32` words whose top five bits are the operator and
//! whose low 27 bits are the operand. The odd back-and-forward markers of the donor layout
//! are preserved; the engine walks the same words.

use alloc::vec::Vec;

use crate::consts::{MAGIC2, OPDMASK, OPRMASK, OPSHIFT};
use crate::locale::{RegexLocale, Wctype};
use crate::utils::uch;

/// One strip word.
pub type Sop = u32;
/// A strip index.
pub type Sopno = u32;

/// `OP(n)`: the operator field.
#[must_use]
pub const fn op(n: Sop) -> u32 {
    n & OPRMASK
}

/// `OPND(n)`: the operand field.
#[must_use]
pub const fn opnd(n: Sop) -> usize {
    (n & OPDMASK) as usize
}

/// `SOP(op, opnd)`: assemble a strip word.
#[must_use]
pub const fn sop(operator: u32, operand: usize) -> Sop {
    operator | (operand as u32 & OPDMASK)
}

/// End marker.
pub const OEND: u32 = 1 << OPSHIFT;
/// One character; the operand is the code point.
pub const OCHAR: u32 = 2 << OPSHIFT;
/// Left anchor.
pub const OBOL: u32 = 3 << OPSHIFT;
/// Right anchor.
pub const OEOL: u32 = 4 << OPSHIFT;
/// `.`.
pub const OANY: u32 = 5 << OPSHIFT;
/// `[...]`; the operand is the set number.
pub const OANYOF: u32 = 6 << OPSHIFT;
/// Begin of a back-reference.
pub const OBACK_: u32 = 7 << OPSHIFT;
/// End of a back-reference.
pub const O_BACK: u32 = 8 << OPSHIFT;
/// `+` prefix; forward to the suffix.
pub const OPLUS_: u32 = 9 << OPSHIFT;
/// `+` suffix; back to the prefix.
pub const O_PLUS: u32 = 10 << OPSHIFT;
/// `?` prefix; forward to the suffix.
pub const OQUEST_: u32 = 11 << OPSHIFT;
/// `?` suffix; back to the prefix.
pub const O_QUEST: u32 = 12 << OPSHIFT;
/// `(`; forward to the `)`.
pub const OLPAREN: u32 = 13 << OPSHIFT;
/// `)`; back to the `(`.
pub const ORPAREN: u32 = 14 << OPSHIFT;
/// Begin of a choice; forward to the first `OOR2`.
pub const OCH_: u32 = 15 << OPSHIFT;
/// `|`, first part; back to `OOR1` or `OCH_`.
pub const OOR1: u32 = 16 << OPSHIFT;
/// `|`, second part; forward to `OOR2` or `O_CH`.
pub const OOR2: u32 = 17 << OPSHIFT;
/// End of a choice; back to `OOR1`.
pub const O_CH: u32 = 18 << OPSHIFT;
/// Begin of a word.
pub const OBOW: u32 = 19 << OPSHIFT;
/// End of a word.
pub const OEOW: u32 = 20 << OPSHIFT;
/// Begin of the subject.
pub const OBOS: u32 = 21 << OPSHIFT;
/// End of the subject.
pub const OEOS: u32 = 22 << OPSHIFT;
/// Word boundary.
pub const OWBND: u32 = 23 << OPSHIFT;
/// Not a word boundary.
pub const ONWBND: u32 = 24 << OPSHIFT;

/// A character-set range, inclusive at both ends.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct CsetRange {
    /// First code point.
    pub min: u32,
    /// Last code point.
    pub max: u32,
}

/// A bracket expression.
#[derive(Clone, Default)]
pub struct Cset {
    /// Bitmap for code points below `NC`.
    pub bmp: [u8; 32],
    /// Wide character classes added by `[[:class:]]`.
    pub types: Vec<Wctype>,
    /// Wide code points added one by one.
    pub wides: Vec<u32>,
    /// Code-point ranges added for `MB_CUR_MAX > 1`.
    pub ranges: Vec<CsetRange>,
    /// `^` was present.
    pub invert: bool,
    /// `REG_ICASE` was in effect.
    pub icase: bool,
}

impl Cset {
    /// `CHIN1`: the slow path of `CHIN`, including the bitmap test.
    #[must_use]
    pub fn chin1(&self, ch: i32, loc: &dyn RegexLocale, nc: usize) -> bool {
        if (ch as u32) < nc as u32 {
            let index = (ch as u32 >> 3) as usize;
            let mask = 1u8 << (ch as u32 & 7);
            let inside = self.bmp.get(index).is_some_and(|byte| byte & mask != 0);
            return inside ^ self.invert;
        }
        for &wide in &self.wides {
            if self.icase {
                let lower = loc.towlower(wide) as i32;
                let upper = loc.towupper(wide) as i32;
                if ch == lower || ch == upper {
                    return !self.invert;
                }
            } else if ch == wide as i32 {
                return !self.invert;
            }
        }
        for range in &self.ranges {
            if range.min as i32 <= ch && ch <= range.max as i32 {
                return !self.invert;
            }
        }
        for &class in &self.types {
            if loc.iswctype(ch as u32, class) {
                return !self.invert;
            }
        }
        self.invert
    }

    /// `CHIN`: test one character, honouring `REG_ICASE`.
    #[must_use]
    pub fn chin(&self, ch: i32, loc: &dyn RegexLocale, nc: usize) -> bool {
        if (ch as u32) < nc as u32 {
            let index = (ch as u32 >> 3) as usize;
            let mask = 1u8 << (ch as u32 & 7);
            let inside = self.bmp.get(index).is_some_and(|byte| byte & mask != 0);
            return inside ^ self.invert;
        }
        if self.icase {
            self.chin1(ch, loc, nc)
                || self.chin1(loc.towlower(ch as u32) as i32, loc, nc)
                || self.chin1(loc.towupper(ch as u32) as i32, loc, nc)
        } else {
            self.chin1(ch, loc, nc)
        }
    }
}

/// The compiled-expression structure (`struct re_guts`).
#[derive(Clone)]
pub struct ReGuts {
    /// `MAGIC2` while the structure is valid.
    pub magic: i32,
    /// The strip.
    pub strip: Vec<Sop>,
    /// Number of sets in use.
    pub ncsets: usize,
    /// The bracket expressions.
    pub sets: Vec<Cset>,
    /// Copy of the `regcomp` cflags argument.
    pub cflags: u32,
    /// Number of strip words.
    pub nstates: Sopno,
    /// The initial `OEND`.
    pub firststate: Sopno,
    /// The final `OEND`.
    pub laststate: Sopno,
    /// Internal flags ([`crate::consts::USEBOL`], [`crate::consts::USEEOL`], [`crate::consts::BAD`]).
    pub iflags: i32,
    /// Number of `^` used.
    pub nbol: usize,
    /// Number of `$` used.
    pub neol: usize,
    /// The longest mandatory literal string, as bytes.
    pub must: Option<Vec<u8>>,
    /// Latest point at which `must` may be located.
    pub moffset: i32,
    /// Boyer-Moore character jump table, 256 entries.
    pub charjump: Option<Vec<usize>>,
    /// Boyer-Moore match jump table, `mlen` entries.
    pub matchjump: Option<Vec<usize>>,
    /// Length of `must`.
    pub mlen: usize,
    /// Copy of `re_nsub`.
    pub nsub: usize,
    /// Whether the pattern uses a back-reference.
    pub backrefs: bool,
    /// How deeply `+` nests.
    pub nplus: Sopno,
}

impl Default for ReGuts {
    fn default() -> Self {
        Self {
            magic: 0,
            strip: Vec::new(),
            ncsets: 0,
            sets: Vec::new(),
            cflags: 0,
            nstates: 0,
            firststate: 0,
            laststate: 0,
            iflags: 0,
            nbol: 0,
            neol: 0,
            must: None,
            moffset: -1,
            charjump: None,
            matchjump: None,
            mlen: 0,
            nsub: 0,
            backrefs: false,
            nplus: 0,
        }
    }
}

impl ReGuts {
    /// A strip word by index; an out-of-range index is a defensive `OEND`.
    #[must_use]
    pub fn word(&self, index: usize) -> Sop {
        self.strip.get(index).copied().unwrap_or(OEND)
    }

    /// `MAGIC2` present?
    #[must_use]
    pub fn magic_ok(&self) -> bool {
        self.magic == MAGIC2
    }
}

/// `ISWORD(c)`: word character for the boundary operators.
///
/// Donor quirk kept: `(uch)(c)` truncates to a byte before `iswalnum`, so code points above
/// 255 are classified through their low byte (see the C45 report, `RD-new`).
#[must_use]
pub fn isword(c: i32, loc: &dyn RegexLocale) -> bool {
    // RavenBSD: donor quirk kept, see report.
    loc.iswalnum(u32::from(uch(c))) || c == i32::from(b'_')
}

#[cfg(test)]
#[path = "regex2_ut.rs"]
mod tests;
