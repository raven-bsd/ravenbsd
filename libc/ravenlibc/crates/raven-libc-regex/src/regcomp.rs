// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1992, 1993, 1994 Henry Spencer.
// Copyright (c) 1992, 1993, 1994
//	The Regents of the University of California.  All rights reserved.
//
// Copyright (c) 2011 The FreeBSD Foundation
// All rights reserved.
// Portions of this software were developed by David Chisnall
// under sponsorship from the FreeBSD Foundation.
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
//	@(#)regcomp.c	8.5 (Berkeley) 3/20/94
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/regex/regcomp.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// Altered: transpiled to Rust; hardening limits MAX_STRIP, BACKREF_BUDGET, MAX_PAREN_DEPTH, BACKREF_DEPTH_MAX; generic state-set engine; wide-character access through RegexLocale.

//! The pattern compiler (`regcomp.c`).
//!
//! Function names and the order of the C file are kept (`p_re`, `p_ere_exp`, `p_simp_re`,
//! `p_bracket`, `repeat`, `dupl`, `doemit`, `enlarge`, `findmust`, `pluscount`, ...) so a
//! reviewer can diff against the donor. The NetBSD build defines `POSIX_MISTAKE`, so an
//! unmatched `)` is an ordinary character; `REGEX_GNU_EXTENSIONS` is compiled in but its
//! escapes are only live when [`crate::consts::REG_GNU`] is passed. `REGEX_LIBC_COLLATE`,
//! `REDEBUG`, `REG_DUMP`, the `NLS`-off tables and the `notyet` blocks are deleted.
//!
//! Hardening: `enlarge` refuses a strip larger than [`MAX_STRIP`] and every `sopno`
//! computation is checked (H-RE-1); nested groups deeper than [`MAX_PAREN_DEPTH`] are
//! [`REG_ESPACE`] (H-RE-3).

use alloc::vec::Vec;

use crate::RegError;
use crate::Regex;
use crate::cname;
use crate::consts::{
    DUPMAX, INFINITY, MAGIC1, MAGIC2, MAX_PAREN_DEPTH, MAX_STRIP, NPAREN, OPDMASK, REG_ASSERT,
    REG_BADBR, REG_BADRPT, REG_EBRACE, REG_EBRACK, REG_ECOLLATE, REG_ECTYPE, REG_EESCAPE,
    REG_EMPTY, REG_EPAREN, REG_ERANGE, REG_ESPACE, REG_ESUBREG, REG_EXTENDED, REG_GNU, REG_ICASE,
    REG_ILLSEQ, REG_INVARG, REG_NEWLINE, REG_NOSPEC, USEBOL, USEEOL,
};
use crate::locale::{RegexLocale, Wctype};
use crate::regex2::{
    Cset, CsetRange, O_BACK, O_CH, O_PLUS, O_QUEST, OANY, OANYOF, OBACK_, OBOL, OBOS, OCH_, OCHAR,
    OEND, OEOL, OEOS, OLPAREN, OOR1, OOR2, OPLUS_, OQUEST_, ORPAREN, OWBND, ReGuts, Sop, Sopno, op,
    opnd, sop,
};
use crate::utils::{IGN, OUT, nc};

/// `struct branchc`: the branch bookkeeping of `p_re`.
struct Branchc {
    /// Strip position where the branch starts.
    start: Sopno,
    /// Backward target of the next `OOR1`.
    back: Sopno,
    /// Forward target of the last `OOR2`.
    fwd: Sopno,
    /// Number of branches emitted.
    nbranch: i32,
    /// Number of expressions in the current branch.
    nchain: i32,
    /// This is the outermost `p_re` call.
    outer: bool,
    /// The last expression ended at an unescaped `$` (BRE only).
    terminate: bool,
}

/// One saved input view for `bothcases`/`nonnewline`.
#[derive(Clone, Copy)]
struct Local {
    /// Synthetic pattern bytes.
    buf: [u8; 24],
    /// Number of valid bytes.
    len: usize,
}

/// The pattern cursor: the real slice, or a synthetic local bracket program.
struct Input<'a> {
    /// The real pattern.
    main: &'a [u8],
    /// Next byte.
    next: usize,
    /// End of the active view.
    end: usize,
    /// The synthetic view when one is active.
    local: Option<Local>,
}

impl<'a> Input<'a> {
    /// A cursor over the whole pattern.
    fn new(main: &'a [u8]) -> Self {
        Self {
            main,
            next: 0,
            end: main.len(),
            local: None,
        }
    }

    /// The active byte storage.
    fn bytes(&self) -> &[u8] {
        match &self.local {
            Some(local) => local.buf.get(..local.len).unwrap_or(&[]),
            None => self.main,
        }
    }

    /// `MORE()`.
    fn more(&self) -> bool {
        self.next < self.end
    }

    /// `MORE2()`.
    fn more2(&self) -> bool {
        self.next
            .checked_add(1)
            .is_some_and(|index| index < self.end)
    }

    /// `PEEK()`; a byte past the end reads as NUL, as the donor's halted cursor does.
    fn peek(&self) -> u8 {
        self.bytes().get(self.next).copied().unwrap_or(0)
    }

    /// `PEEK2()`.
    fn peek2(&self) -> u8 {
        self.next
            .checked_add(1)
            .and_then(|index| self.bytes().get(index))
            .copied()
            .unwrap_or(0)
    }

    /// `NEXT()`.
    fn advance(&mut self) {
        if self.more() {
            self.next = self.next.wrapping_add(1);
        }
    }

    /// `NEXT2()`.
    fn advance2(&mut self) {
        self.advance();
        self.advance();
    }

    /// `NEXTn(n)`.
    fn advance_n(&mut self, count: usize) {
        for _ in 0..count {
            self.advance();
        }
    }

    /// `p->next--`.
    fn back(&mut self) {
        self.next = self.next.saturating_sub(1);
    }

    /// `GETNEXT()`.
    fn get_next(&mut self) -> u8 {
        let byte = self.peek();
        self.advance();
        byte
    }

    /// The bytes from the cursor to the end of the view.
    fn remaining(&self) -> &[u8] {
        self.bytes().get(self.next..self.end).unwrap_or(&[])
    }

    /// A byte range of the active view.
    fn slice(&self, start: usize, end: usize) -> &[u8] {
        self.bytes().get(start..end).unwrap_or(&[])
    }

    /// The cursor offset.
    fn pos(&self) -> usize {
        self.next
    }

    /// `seterr`'s "bring things to a halt": point at an empty input.
    fn halt(&mut self) {
        self.next = self.end;
    }

    /// Enter a synthetic bracket view; returns the state to restore.
    fn push_local(&mut self, data: &[u8]) -> Option<(usize, usize, Option<Local>)> {
        let mut buf = [0u8; 24];
        if data.len() > buf.len() {
            return None;
        }
        for (slot, &byte) in buf.iter_mut().zip(data.iter()) {
            *slot = byte;
        }
        let saved = (self.next, self.end, self.local);
        self.local = Some(Local {
            buf,
            len: data.len(),
        });
        self.next = 0;
        self.end = data.len();
        Some(saved)
    }

    /// Leave a synthetic view.
    fn pop_local(&mut self, saved: (usize, usize, Option<Local>)) {
        self.next = saved.0;
        self.end = saved.1;
        self.local = saved.2;
    }
}

/// `struct parse`: the compiler state.
struct Parse<'a> {
    /// Pattern cursor.
    input: Input<'a>,
    /// First error seen (`seterr` keeps the earliest).
    error: i32,
    /// GNU escapes are enabled (`REG_GNU`).
    gnuext: bool,
    /// The strip under construction.
    strip: Vec<Sop>,
    /// Strip capacity the donor tracks in `ssize`.
    ssize: usize,
    /// The guts under construction.
    g: ReGuts,
    /// `(` positions for back-references (`[0]` unused).
    pbegin: [Sopno; NPAREN],
    /// `)` positions for back-references (`[0]` unused).
    pend: [Sopno; NPAREN],
    /// The expression may branch (`|`).
    allowbranch: bool,
    /// The pattern is a BRE.
    bre: bool,
    /// Nested group depth (H-RE-3).
    depth: usize,
    /// The locale seam.
    loc: &'a dyn RegexLocale,
    /// `NC` of the compile locale.
    nc: usize,
    /// `MB_CUR_MAX` of the compile locale.
    mb_cur_max: usize,
}

/// `HERE()`.
fn here(p: &Parse) -> Sopno {
    p.strip.len() as Sopno
}

/// `DROP(n)`.
fn drop_strip(p: &mut Parse, count: usize) {
    let keep = p.strip.len().saturating_sub(count);
    p.strip.truncate(keep);
}

/// `SETERROR(e)`; returns `false` like the donor's `seterr`.
fn seterror(p: &mut Parse, error: i32) -> bool {
    if p.error == 0 {
        p.error = error;
    }
    p.input.halt();
    false
}

/// `REQUIRE(co, e)`.
fn require(p: &mut Parse, condition: bool, error: i32) -> bool {
    condition || seterror(p, error)
}

/// `REQUIRE(MORE(), e)` without borrowing the parse twice.
fn require_more(p: &mut Parse, error: i32) -> bool {
    let more = p.input.more();
    require(p, more, error)
}

/// `SEE(c)`.
fn see(p: &Parse, c: i32) -> bool {
    if !(0..=255).contains(&c) {
        return false;
    }
    p.input.more() && p.input.peek() == c as u8
}

/// `SEETWO(a, b)`.
fn seetwo(p: &Parse, a: i32, b: i32) -> bool {
    if !(0..=255).contains(&a) || !(0..=255).contains(&b) {
        return false;
    }
    p.input.more() && p.input.more2() && p.input.peek() == a as u8 && p.input.peek2() == b as u8
}

/// `SEESPEC(a)`.
fn seespec(p: &Parse, c: i32) -> bool {
    if p.bre {
        seetwo(p, i32::from(b'\\'), c)
    } else {
        see(p, c)
    }
}

/// `EAT(c)`.
fn eat(p: &mut Parse, c: i32) -> bool {
    if see(p, c) {
        p.input.advance();
        true
    } else {
        false
    }
}

/// `EATTWO(a, b)`.
fn eattwo(p: &mut Parse, a: i32, b: i32) -> bool {
    if seetwo(p, a, b) {
        p.input.advance2();
        true
    } else {
        false
    }
}

/// `EATSPEC(a)`.
fn eatspec(p: &mut Parse, c: i32) -> bool {
    if p.bre {
        eattwo(p, i32::from(b'\\'), c)
    } else {
        eat(p, c)
    }
}

/// `SETERROR` for defensive `assert` replacements.
fn bad_assert(p: &mut Parse) {
    seterror(p, REG_ASSERT);
}

/// `enlarge`: grow the strip, never past [`MAX_STRIP`].
fn enlarge(p: &mut Parse, size: usize) -> bool {
    if p.ssize >= size {
        return true;
    }
    if size > MAX_STRIP {
        seterror(p, REG_ESPACE);
        return false;
    }
    if p.strip
        .try_reserve(size.saturating_sub(p.strip.len()))
        .is_err()
    {
        seterror(p, REG_ESPACE);
        return false;
    }
    p.ssize = size;
    true
}

/// `doemit`.
fn doemit(p: &mut Parse, operator: u32, operand: usize) {
    if p.error != 0 {
        return;
    }
    if operand > OPDMASK as usize {
        bad_assert(p);
        return;
    }
    if p.strip.len() >= p.ssize {
        let grow = p.ssize.wrapping_add(1).wrapping_div(2).wrapping_mul(3);
        if grow > MAX_STRIP {
            if p.strip.len().saturating_add(1) > MAX_STRIP {
                seterror(p, REG_ESPACE);
                return;
            }
            if !enlarge(p, MAX_STRIP) {
                return;
            }
        } else if !enlarge(p, grow) {
            return;
        }
    }
    p.strip.push(sop(operator, operand));
}

/// `EMIT(op, opnd)`.
fn emit(p: &mut Parse, operator: u32, operand: usize) {
    doemit(p, operator, operand);
}

/// `doinsert`.
fn doinsert(p: &mut Parse, operator: u32, operand: usize, pos: Sopno) {
    if p.error != 0 {
        return;
    }
    let sn = p.strip.len();
    emit(p, operator, operand);
    if p.error != 0 || p.strip.len() != sn.saturating_add(1) {
        return;
    }
    let Some(word) = p.strip.pop() else {
        return;
    };
    for index in 1..NPAREN {
        if let Some(value) = p.pbegin.get(index).copied()
            && value >= pos
            && let Some(slot) = p.pbegin.get_mut(index)
        {
            *slot = value.saturating_add(1);
        }
        if let Some(value) = p.pend.get(index).copied()
            && value >= pos
            && let Some(slot) = p.pend.get_mut(index)
        {
            *slot = value.saturating_add(1);
        }
    }
    let at = (pos as usize).min(p.strip.len());
    p.strip.insert(at, word);
}

/// `INSERT(op, pos)`.
fn insert(p: &mut Parse, operator: u32, pos: Sopno) {
    let operand = p
        .strip
        .len()
        .checked_sub(pos as usize)
        .and_then(|value| value.checked_add(1))
        .unwrap_or(0);
    doinsert(p, operator, operand, pos);
}

/// `dofwd`.
fn dofwd(p: &mut Parse, pos: Sopno, value: Sopno) {
    if p.error != 0 {
        return;
    }
    if value > OPDMASK {
        bad_assert(p);
        return;
    }
    if let Some(word) = p.strip.get(pos as usize).copied()
        && let Some(slot) = p.strip.get_mut(pos as usize)
    {
        *slot = op(word) | value;
    }
}

/// `AHEAD(pos)`.
fn ahead(p: &mut Parse, pos: Sopno) {
    let value = here(p).saturating_sub(pos);
    dofwd(p, pos, value);
}

/// `ASTERN(sop, pos)`.
fn astern(p: &mut Parse, operator: u32, pos: Sopno) {
    let operand = here(p).saturating_sub(pos);
    emit(p, operator, operand as usize);
}

/// `dupl`: append a copy of `[start, finish)`.
fn dupl(p: &mut Parse, start: Sopno, finish: Sopno) -> Sopno {
    let ret = here(p);
    if finish < start {
        bad_assert(p);
        return ret;
    }
    let Some(len) = (finish as usize).checked_sub(start as usize) else {
        bad_assert(p);
        return ret;
    };
    if len == 0 {
        return ret;
    }
    // RavenBSD: the donor asks for `ssize + len`; the exact needed size keeps the capacity
    // honest and lets the MAX_STRIP boundary be reached (the donor expression can overshoot
    // the cap by itself). The cap check itself still lives in `enlarge`.
    let Some(want) = p.strip.len().checked_add(len) else {
        seterror(p, REG_ESPACE);
        return ret;
    };
    if !enlarge(p, want) {
        return ret;
    }
    let Some(tail) = p.strip.get(start as usize..finish as usize) else {
        bad_assert(p);
        return ret;
    };
    let copy: Vec<Sop> = tail.to_vec();
    if p.strip.try_reserve(len).is_err() {
        seterror(p, REG_ESPACE);
        return ret;
    }
    p.strip.extend_from_slice(&copy);
    ret
}

/// `CHadd`.
fn chadd(p: &mut Parse, cs: &mut Cset, ch: i32) {
    if (ch as u32) < p.nc as u32 {
        let index = (ch as u32 >> 3) as usize;
        let mask = 1u8 << (ch as u32 & 7);
        if let Some(slot) = cs.bmp.get_mut(index) {
            *slot |= mask;
        }
    } else {
        if cs.wides.try_reserve(1).is_err() {
            seterror(p, REG_ESPACE);
            return;
        }
        cs.wides.push(ch as u32);
    }
    if cs.icase {
        let lower = p.loc.towlower(ch as u32) as i32;
        if (lower as u32) < p.nc as u32 {
            let index = (lower as u32 >> 3) as usize;
            let mask = 1u8 << (lower as u32 & 7);
            if let Some(slot) = cs.bmp.get_mut(index) {
                *slot |= mask;
            }
        }
        let upper = p.loc.towupper(ch as u32) as i32;
        if (upper as u32) < p.nc as u32 {
            let index = (upper as u32 >> 3) as usize;
            let mask = 1u8 << (upper as u32 & 7);
            if let Some(slot) = cs.bmp.get_mut(index) {
                *slot |= mask;
            }
        }
    }
}

/// `CHaddrange`.
fn chaddrange(p: &mut Parse, cs: &mut Cset, min: i32, max: i32) {
    let mut start = min;
    while (start as u32) < p.nc as u32 && start <= max {
        chadd(p, cs, start);
        start = start.saturating_add(1);
    }
    if start >= max {
        return;
    }
    if cs.ranges.try_reserve(1).is_err() {
        seterror(p, REG_ESPACE);
        return;
    }
    cs.ranges.push(CsetRange {
        min: start as u32,
        max: max as u32,
    });
}

/// `CHaddtype`.
fn chaddtype(p: &mut Parse, cs: &mut Cset, class: Wctype) {
    let mut index = 0usize;
    while index < p.nc {
        if p.loc.iswctype(index as u32, class) {
            chadd(p, cs, index as i32);
        }
        index = index.wrapping_add(1);
    }
    if cs.types.try_reserve(1).is_err() {
        seterror(p, REG_ESPACE);
        return;
    }
    cs.types.push(class);
}

/// `allocset`: reserve a set slot and return its index.
fn allocset(p: &mut Parse) -> Option<usize> {
    if p.g.sets.try_reserve(1).is_err() {
        seterror(p, REG_ESPACE);
        return None;
    }
    let index = p.g.sets.len();
    p.g.sets.push(Cset::default());
    p.g.ncsets = p.g.sets.len();
    Some(index)
}

/// Take a set out of the guts so it can be mutated without borrowing the parse.
fn take_set(p: &mut Parse, index: usize) -> Cset {
    p.g.sets
        .get_mut(index)
        .map(core::mem::take)
        .unwrap_or_default()
}

/// Put a set back.
fn put_set(p: &mut Parse, index: usize, cs: Cset) {
    if let Some(slot) = p.g.sets.get_mut(index) {
        *slot = cs;
    }
}

/// `freeset`.
fn freeset(p: &mut Parse, index: usize, cs: Cset) {
    let _ = cs;
    if index.saturating_add(1) == p.g.sets.len() {
        p.g.sets.pop();
    } else if let Some(slot) = p.g.sets.get_mut(index) {
        *slot = Cset::default();
    }
    p.g.ncsets = p.g.sets.len();
}

/// `singleton`: the only character of a set, when there is exactly one.
fn singleton(cs: &Cset, loc: &dyn RegexLocale, nc_value: usize) -> Option<i32> {
    let mut found: Option<i32> = None;
    let mut count = 0i32;
    for index in 0..nc_value {
        if cs.chin(index as i32, loc, nc_value) {
            count = count.saturating_add(1);
            found = Some(index as i32);
        }
    }
    if count == 1 {
        return found;
    }
    if cs.wides.len() == 1 && cs.ranges.is_empty() && cs.types.is_empty() && !cs.icase {
        return cs.wides.first().map(|&value| value as i32);
    }
    None
}

/// Encode one code point for the active locale (the donor's `wcrtomb`).
fn encode(p: &Parse, ch: i32, out: &mut [u8; 8]) -> Option<usize> {
    if p.mb_cur_max == 1 {
        if !(0..=255).contains(&ch) {
            return None;
        }
        out[0] = ch as u8;
        return Some(1);
    }
    let character = char::from_u32(ch as u32)?;
    let mut utf8 = [0u8; 4];
    let encoded = character.encode_utf8(&mut utf8);
    let length = encoded.len();
    for (slot, &byte) in out.iter_mut().zip(utf8.iter()).take(length) {
        *slot = byte;
    }
    Some(length)
}

/// `wgetnext`: decode one pattern character, `REG_ILLSEQ` on a bad sequence.
fn wgetnext(p: &mut Parse) -> i32 {
    let mut state = raven_libc_ctype::MbState::INITIAL;
    let result = {
        let remaining = p.input.remaining();
        p.loc.mbrtowc(remaining, &mut state)
    };
    match result {
        crate::locale::MbResult::Ok(wc, used) => {
            let step = if used == 0 { 1 } else { used };
            p.input.advance_n(step);
            wc as i32
        }
        crate::locale::MbResult::Incomplete | crate::locale::MbResult::Invalid => {
            seterror(p, REG_ILLSEQ);
            0
        }
    }
}

/// `p_str`: the `REG_NOSPEC` "parser".
fn p_str(p: &mut Parse) {
    let _ = require_more(p, REG_EMPTY);
    while p.input.more() {
        let ch = wgetnext(p);
        if p.error != 0 {
            return;
        }
        ordinary(p, ch);
    }
}

/// `p_re`: top-level parser, concatenation and BRE anchoring.
fn p_re(p: &mut Parse, end1: i32, end2: i32) {
    let mut bc = Branchc {
        start: 0,
        back: 0,
        fwd: 0,
        nbranch: 0,
        nchain: 0,
        outer: end1 == OUT && end2 == OUT,
        terminate: false,
    };
    loop {
        bc.start = here(p);
        bc.nchain = 0;
        bc.terminate = false;
        if p.bre {
            p_bre_pre_parse(p, &mut bc);
        }
        while p.input.more() && (!p.allowbranch || !seespec(p, i32::from(b'|'))) {
            let end = if p.bre {
                seetwo(p, end1, end2)
            } else {
                see(p, end1)
            };
            if end {
                break;
            }
            bc.terminate = if p.bre {
                p_simp_re(p, &mut bc)
            } else {
                p_ere_exp(p, &mut bc)
            };
            bc.nchain = bc.nchain.saturating_add(1);
        }
        if p.bre {
            p_bre_post_parse(p, &mut bc);
        }
        let nonempty = p.gnuext || here(p) != bc.start;
        let _ = require(p, nonempty, REG_EMPTY);
        if p.gnuext && here(p) == bc.start {
            let _ = p_branch_empty(p, &mut bc);
            break;
        }
        if !p.allowbranch {
            break;
        }
        if !p_branch_do(p, &mut bc) {
            break;
        }
        if p.error != 0 {
            break;
        }
    }
    if p.allowbranch {
        p_branch_fix_tail(p, &mut bc);
    }
}

/// `p_ere_exp`.
fn p_ere_exp(p: &mut Parse, bc: &mut Branchc) -> bool {
    let c = p.input.get_next() as i32;
    let mut handled = false;
    let pos = here(p);
    let mut wascaret = false;
    match c {
        x if x == i32::from(b'(') => {
            let _ = require_more(p, REG_EPAREN);
            p.g.nsub = p.g.nsub.saturating_add(1);
            let subno = p.g.nsub as Sopno;
            let beginpos = here(p);
            if (subno as usize) < NPAREN
                && let Some(slot) = p.pbegin.get_mut(subno as usize)
            {
                *slot = beginpos;
            }
            emit(p, OLPAREN, subno as usize);
            if !see(p, i32::from(b')')) && p.error == 0 && enter_group(p) {
                p_re(p, i32::from(b')'), IGN);
                leave_group(p);
            }
            let endpos = here(p);
            if (subno as usize) < NPAREN
                && let Some(slot) = p.pend.get_mut(subno as usize)
            {
                *slot = endpos;
            }
            emit(p, ORPAREN, subno as usize);
            let _ = require(p, p.input.more() && p.input.peek() == b')', REG_EPAREN);
            if p.input.peek() == b')' && p.input.more() {
                p.input.advance();
            }
        }
        x if x == i32::from(b'^') => {
            emit(p, OBOL, 0);
            p.g.iflags |= USEBOL;
            p.g.nbol = p.g.nbol.saturating_add(1);
            wascaret = true;
        }
        x if x == i32::from(b'$') => {
            emit(p, OEOL, 0);
            p.g.iflags |= USEEOL;
            p.g.neol = p.g.neol.saturating_add(1);
        }
        x if x == i32::from(b'|') => {
            let _ = seterror(p, REG_EMPTY);
        }
        x if x == i32::from(b'*')
            || x == i32::from(b'+')
            || x == i32::from(b'?')
            || x == i32::from(b'{') =>
        {
            let _ = seterror(p, REG_BADRPT);
        }
        x if x == i32::from(b'.') => {
            if p.g.cflags & REG_NEWLINE != 0 {
                nonnewline(p);
            } else {
                emit(p, OANY, 0);
            }
        }
        x if x == i32::from(b'[') => {
            p_bracket(p);
        }
        x if x == i32::from(b'\\') => {
            let _ = require_more(p, REG_EESCAPE);
            let wc = wgetnext(p);
            if p.error != 0 {
                return false;
            }
            if p.gnuext {
                handled = true;
                match wc {
                    w if w == i32::from(b'`') => emit(p, OBOS, 0),
                    w if w == i32::from(b'\'') => emit(p, OEOS, 0),
                    w if w == i32::from(b'B') => emit(p, crate::regex2::ONWBND, 0),
                    w if w == i32::from(b'b') => emit(p, crate::regex2::OWBND, 0),
                    w if w == i32::from(b'W')
                        || w == i32::from(b'w')
                        || w == i32::from(b'S')
                        || w == i32::from(b's') =>
                    {
                        p_b_pseudoclass(p, w);
                    }
                    w if w == i32::from(b'a') => ordinary(p, 7),
                    w if w == i32::from(b'e') => ordinary(p, 0x1b),
                    w if w == i32::from(b'f') => ordinary(p, 12),
                    w if w == i32::from(b'n') => ordinary(p, 10),
                    w if w == i32::from(b'r') => ordinary(p, 13),
                    w if w == i32::from(b't') => ordinary(p, 9),
                    w if w == i32::from(b'v') => ordinary(p, 11),
                    w if (i32::from(b'1')..=i32::from(b'9')).contains(&w) => {
                        let index = w.wrapping_sub(i32::from(b'0')) as usize;
                        if p.pend.get(index).copied().unwrap_or(0) != 0 {
                            emit(p, OBACK_, index);
                            let begin = p.pbegin.get(index).copied().unwrap_or(0);
                            let finish = p.pend.get(index).copied().unwrap_or(0);
                            if begin == 0 || finish == 0 {
                                bad_assert(p);
                            } else {
                                let _ = dupl(p, begin.saturating_add(1), finish);
                                emit(p, O_BACK, index);
                            }
                        } else {
                            let _ = seterror(p, REG_ESUBREG);
                        }
                        p.g.backrefs = true;
                    }
                    _ => handled = false,
                }
            }
            if !handled && p.error == 0 {
                match wc {
                    w if w == i32::from(b'<') => emit(p, OWBND, 0),
                    w if w == i32::from(b'>') => emit(p, crate::regex2::OEOW, 0),
                    _ => {
                        if may_escape(p, wc) {
                            ordinary(p, wc);
                        } else {
                            let _ = seterror(p, REG_EESCAPE);
                        }
                    }
                }
            }
        }
        _ => {
            if p.error != 0 {
                return false;
            }
            p.input.back();
            let wc = wgetnext(p);
            if p.error != 0 {
                return false;
            }
            ordinary(p, wc);
        }
    }
    let _ = bc;

    if !p.input.more() {
        return false;
    }
    let mut next = p.input.peek() as i32;
    if !(next == i32::from(b'*')
        || next == i32::from(b'+')
        || next == i32::from(b'?')
        || next == i32::from(b'{'))
    {
        return false;
    } else if next == i32::from(b'{') {
        let valid =
            p.input.more2() && (p.input.peek2().is_ascii_digit() || p.input.peek2() == b',');
        let _ = require(p, valid, REG_BADRPT);
    }
    p.input.advance();

    let _ = require(p, !wascaret, REG_BADRPT);
    match next {
        x if x == i32::from(b'*') => {
            insert(p, OPLUS_, pos);
            astern(p, O_PLUS, pos);
            insert(p, OQUEST_, pos);
            astern(p, O_QUEST, pos);
        }
        x if x == i32::from(b'+') => {
            insert(p, OPLUS_, pos);
            astern(p, O_PLUS, pos);
        }
        x if x == i32::from(b'?') => {
            insert(p, OCH_, pos);
            astern(p, OOR1, pos);
            ahead(p, pos);
            emit(p, OOR2, 0);
            ahead(p, here(p).saturating_sub(1));
            astern(p, O_CH, here(p).saturating_sub(2));
        }
        x if x == i32::from(b'{') => {
            let count = p_count(p);
            let count2;
            if eat(p, i32::from(b',')) {
                if p.input.peek().is_ascii_digit() {
                    let second = p_count(p);
                    let _ = require(p, count <= second, REG_BADBR);
                    count2 = second;
                } else {
                    count2 = INFINITY;
                }
            } else {
                count2 = count;
            }
            repeat(p, pos, count, count2);
            if !eat(p, i32::from(b'}')) {
                while p.input.more() && p.input.peek() != b'}' {
                    p.input.advance();
                }
                let _ = require_more(p, REG_EBRACE);
                let _ = seterror(p, REG_BADBR);
            }
        }
        _ => {}
    }

    if !p.input.more() {
        return false;
    }
    next = p.input.peek() as i32;
    let repeated = next == i32::from(b'*')
        || next == i32::from(b'+')
        || next == i32::from(b'?')
        || (next == i32::from(b'{') && p.input.more2() && p.input.peek2().is_ascii_digit());
    if !repeated {
        return false;
    }
    let _ = seterror(p, REG_BADRPT);
    false
}

/// `enter_group`: H-RE-3 depth accounting.
fn enter_group(p: &mut Parse) -> bool {
    p.depth = p.depth.saturating_add(1);
    if p.depth > MAX_PAREN_DEPTH {
        let _ = seterror(p, REG_ESPACE);
        return false;
    }
    true
}

/// `leave_group`: undo [`enter_group`].
fn leave_group(p: &mut Parse) {
    p.depth = p.depth.saturating_sub(1);
}

/// `p_simp_re`.
fn p_simp_re(p: &mut Parse, bc: &mut Branchc) -> bool {
    const BACKSL: i32 = 1 << 8;
    let pos = here(p);
    let mut handled = false;
    let mut c = i32::from(p.input.get_next());
    if c == i32::from(b'\\') {
        let _ = require_more(p, REG_EESCAPE);
        let cc = i32::from(p.input.get_next());
        c = BACKSL | cc;
        if p.gnuext && p.error == 0 {
            handled = true;
            match c {
                x if x == BACKSL | i32::from(b'`') => emit(p, OBOS, 0),
                x if x == BACKSL | i32::from(b'\'') => emit(p, OEOS, 0),
                x if x == BACKSL | i32::from(b'B') => emit(p, crate::regex2::ONWBND, 0),
                x if x == BACKSL | i32::from(b'b') => emit(p, OWBND, 0),
                x if x == BACKSL | i32::from(b'W')
                    || x == BACKSL | i32::from(b'w')
                    || x == BACKSL | i32::from(b'S')
                    || x == BACKSL | i32::from(b's') =>
                {
                    let wc = x & !BACKSL;
                    p_b_pseudoclass(p, wc);
                }
                x if x == BACKSL | i32::from(b'a') => ordinary(p, 7),
                x if x == BACKSL | i32::from(b'e') => ordinary(p, 0x1b),
                x if x == BACKSL | i32::from(b'f') => ordinary(p, 12),
                x if x == BACKSL | i32::from(b'n') => ordinary(p, 10),
                x if x == BACKSL | i32::from(b'r') => ordinary(p, 13),
                x if x == BACKSL | i32::from(b't') => ordinary(p, 9),
                x if x == BACKSL | i32::from(b'v') => ordinary(p, 11),
                _ => handled = false,
            }
        }
    }
    if !handled && p.error == 0 {
        match c {
            x if x == i32::from(b'.') => {
                if p.g.cflags & REG_NEWLINE != 0 {
                    nonnewline(p);
                } else {
                    emit(p, OANY, 0);
                }
            }
            x if x == i32::from(b'[') => p_bracket(p),
            x if x == BACKSL | i32::from(b'<') => emit(p, OWBND, 0),
            x if x == BACKSL | i32::from(b'>') => emit(p, crate::regex2::OEOW, 0),
            x if x == BACKSL | i32::from(b'{') => {
                let _ = seterror(p, REG_BADRPT);
            }
            x if x == BACKSL | i32::from(b'(') => {
                p.g.nsub = p.g.nsub.saturating_add(1);
                let subno = p.g.nsub as Sopno;
                let beginpos = here(p);
                if (subno as usize) < NPAREN
                    && let Some(slot) = p.pbegin.get_mut(subno as usize)
                {
                    *slot = beginpos;
                }
                emit(p, OLPAREN, subno as usize);
                if p.input.more() && !seetwo(p, i32::from(b'\\'), i32::from(b')')) && enter_group(p)
                {
                    p_re(p, i32::from(b'\\'), i32::from(b')'));
                    leave_group(p);
                }
                let endpos = here(p);
                if (subno as usize) < NPAREN
                    && let Some(slot) = p.pend.get_mut(subno as usize)
                {
                    *slot = endpos;
                }
                emit(p, ORPAREN, subno as usize);
                let _ = require(
                    p,
                    p.input.more()
                        && p.input.peek() == b'\\'
                        && p.input.more2()
                        && p.input.peek2() == b')',
                    REG_EPAREN,
                );
                if p.input.more()
                    && p.input.peek() == b'\\'
                    && p.input.more2()
                    && p.input.peek2() == b')'
                {
                    p.input.advance2();
                }
            }
            x if x == BACKSL | i32::from(b')') => {
                let _ = seterror(p, REG_EPAREN);
            }
            x if (BACKSL | i32::from(b'1')) <= x && x <= (BACKSL | i32::from(b'9')) => {
                let index = usize::from((x & !BACKSL) as u8).wrapping_sub(usize::from(b'0'));
                if p.pend.get(index).copied().unwrap_or(0) != 0 {
                    emit(p, OBACK_, index);
                    let begin = p.pbegin.get(index).copied().unwrap_or(0);
                    let finish = p.pend.get(index).copied().unwrap_or(0);
                    if begin == 0 || finish == 0 {
                        bad_assert(p);
                    } else {
                        let _ = dupl(p, begin.saturating_add(1), finish);
                        emit(p, O_BACK, index);
                    }
                } else {
                    let _ = seterror(p, REG_ESUBREG);
                }
                p.g.backrefs = true;
            }
            x if x == i32::from(b'*') => {
                let _ = require(p, bc.nchain == 0, REG_BADRPT);
                if p.error != 0 {
                    return false;
                }
                p.input.back();
                let wc = wgetnext(p);
                if p.error != 0 {
                    return false;
                }
                ordinary(p, wc);
            }
            _ => {
                if p.error != 0 {
                    return false;
                }
                p.input.back();
                let wc = wgetnext(p);
                if p.error != 0 {
                    return false;
                }
                if c & BACKSL == 0 || may_escape(p, wc) {
                    ordinary(p, wc);
                } else {
                    let _ = seterror(p, REG_EESCAPE);
                }
            }
        }
    }

    if p.error == 0 && eat(p, i32::from(b'*')) {
        insert(p, OPLUS_, pos);
        astern(p, O_PLUS, pos);
        insert(p, OQUEST_, pos);
        astern(p, O_QUEST, pos);
    } else if p.error == 0 && p.gnuext && eattwo(p, i32::from(b'\\'), i32::from(b'?')) {
        insert(p, OQUEST_, pos);
        astern(p, O_QUEST, pos);
    } else if p.error == 0 && p.gnuext && eattwo(p, i32::from(b'\\'), i32::from(b'+')) {
        insert(p, OPLUS_, pos);
        astern(p, O_PLUS, pos);
    } else if p.error == 0 && eattwo(p, i32::from(b'\\'), i32::from(b'{')) {
        let count = p_count(p);
        let count2;
        if eat(p, i32::from(b',')) {
            if p.input.more() && p.input.peek().is_ascii_digit() {
                let second = p_count(p);
                let _ = require(p, count <= second, REG_BADBR);
                count2 = second;
            } else {
                count2 = INFINITY;
            }
        } else {
            count2 = count;
        }
        repeat(p, pos, count, count2);
        if !(p.input.more()
            && p.input.peek() == b'\\'
            && p.input.more2()
            && p.input.peek2() == b'}')
        {
            while p.input.more() && !seetwo(p, i32::from(b'\\'), i32::from(b'}')) {
                p.input.advance();
            }
            let _ = require_more(p, REG_EBRACE);
            let _ = seterror(p, REG_BADBR);
        } else {
            p.input.advance2();
        }
    } else if p.error == 0 && c == i32::from(b'$') {
        return true;
    }

    false
}

/// `p_branch_eat_delim`.
fn p_branch_eat_delim(p: &mut Parse, bc: &mut Branchc) -> i32 {
    let _ = bc;
    let mut skipped = 0i32;
    while eatspec(p, i32::from(b'|')) {
        skipped = skipped.saturating_add(1);
    }
    skipped
}

/// `p_branch_ins_offset`.
fn p_branch_ins_offset(p: &mut Parse, bc: &mut Branchc) {
    if bc.nbranch == 0 {
        insert(p, OCH_, bc.start);
        bc.fwd = bc.start;
        bc.back = bc.start;
    }
    astern(p, OOR1, bc.back);
    bc.back = here(p).saturating_sub(1);
    ahead(p, bc.fwd);
    bc.fwd = here(p);
    emit(p, OOR2, 0);
    bc.nbranch = bc.nbranch.saturating_add(1);
}

/// `p_branch_fix_tail`.
fn p_branch_fix_tail(p: &mut Parse, bc: &mut Branchc) {
    if bc.nbranch > 0 {
        ahead(p, bc.fwd);
        astern(p, O_CH, bc.back);
    }
}

/// `p_branch_empty`.
fn p_branch_empty(p: &mut Parse, bc: &mut Branchc) -> bool {
    let _ = bc;
    let _ = seterror(p, REG_EMPTY);
    false
}

/// `p_branch_do`.
fn p_branch_do(p: &mut Parse, bc: &mut Branchc) -> bool {
    let ate = p_branch_eat_delim(p, bc);
    if ate == 0 {
        return false;
    }
    if (ate > 1 || (bc.outer && !p.input.more())) && !p_branch_empty(p, bc) {
        return false;
    }
    p_branch_ins_offset(p, bc);
    true
}

/// `p_bre_pre_parse`.
fn p_bre_pre_parse(p: &mut Parse, bc: &mut Branchc) {
    let _ = bc;
    if eat(p, i32::from(b'^')) {
        emit(p, OBOL, 0);
        p.g.iflags |= USEBOL;
        p.g.nbol = p.g.nbol.saturating_add(1);
    }
}

/// `p_bre_post_parse`.
fn p_bre_post_parse(p: &mut Parse, bc: &mut Branchc) {
    if bc.terminate {
        drop_strip(p, 1);
        emit(p, OEOL, 0);
        p.g.iflags |= USEEOL;
        p.g.neol = p.g.neol.saturating_add(1);
    }
}

/// `p_count`.
fn p_count(p: &mut Parse) -> i32 {
    let mut count = 0i32;
    let mut digits = 0i32;
    while p.input.more() && p.input.peek().is_ascii_digit() && count <= DUPMAX {
        let digit = i32::from(p.input.get_next().wrapping_sub(b'0'));
        count = count.saturating_mul(10).saturating_add(digit);
        digits = digits.saturating_add(1);
    }
    let valid = digits > 0 && count <= DUPMAX;
    let _ = require(p, valid, REG_BADBR);
    count
}

/// `p_range_cmp`: plain code-point comparison (doc 07 4.2; no collation).
fn p_range_cmp(c1: i32, c2: i32) -> i32 {
    c1.saturating_sub(c2)
}

/// `p_bracket`.
fn p_bracket(p: &mut Parse) {
    if p.input.remaining().starts_with(b"[:<:]]") {
        emit(p, OWBND, 0);
        p.input.advance_n(6);
        return;
    }
    if p.input.remaining().starts_with(b"[:>:]]") {
        emit(p, crate::regex2::OEOW, 0);
        p.input.advance_n(6);
        return;
    }
    let Some(index) = allocset(p) else {
        return;
    };
    let mut cs = take_set(p, index);
    if p.g.cflags & REG_ICASE != 0 {
        cs.icase = true;
    }
    if eat(p, i32::from(b'^')) {
        cs.invert = true;
    }
    if eat(p, i32::from(b']')) {
        chadd(p, &mut cs, i32::from(b']'));
    } else if eat(p, i32::from(b'-')) {
        chadd(p, &mut cs, i32::from(b'-'));
    }
    while p.input.more() && p.input.peek() != b']' && !seetwo(p, i32::from(b'-'), i32::from(b']')) {
        p_b_term(p, &mut cs);
        if p.error != 0 {
            break;
        }
    }
    if eat(p, i32::from(b'-')) {
        chadd(p, &mut cs, i32::from(b'-'));
    }
    let _ = require_more(p, REG_EBRACK);
    let closed = p.input.more() && p.input.get_next() == b']';
    let _ = require(p, closed, REG_EBRACK);

    if p.error != 0 {
        put_set(p, index, cs);
        return;
    }

    if cs.invert && p.g.cflags & REG_NEWLINE != 0 {
        let index = usize::from(b'\n') >> 3;
        if let Some(slot) = cs.bmp.get_mut(index) {
            *slot |= 1 << (b'\n' & 7);
        }
    }

    if let Some(ch) = singleton(&cs, p.loc, p.nc) {
        ordinary(p, ch);
        freeset(p, index, cs);
    } else {
        put_set(p, index, cs);
        emit(p, OANYOF, index);
    }
}

/// `p_b_term`.
fn p_b_term(p: &mut Parse, cs: &mut Cset) {
    let mut c = 0u8;
    match if p.input.more() { p.input.peek() } else { 0 } {
        b'[' => {
            if p.input.more2() {
                c = p.input.peek2();
            }
        }
        b'-' => {
            let _ = seterror(p, REG_ERANGE);
            return;
        }
        _ => {}
    }

    match c {
        b':' => {
            p.input.advance2();
            let _ = require_more(p, REG_EBRACK);
            let next = p.input.peek();
            let _ = require(p, next != b'-' && next != b']', REG_ECTYPE);
            p_b_cclass(p, cs);
            let _ = require_more(p, REG_EBRACK);
            let _ = require(p, seetwo(p, i32::from(b':'), i32::from(b']')), REG_ECTYPE);
            if seetwo(p, i32::from(b':'), i32::from(b']')) {
                p.input.advance2();
            }
        }
        b'=' => {
            p.input.advance2();
            let _ = require_more(p, REG_EBRACK);
            let next = p.input.peek();
            let _ = require(p, next != b'-' && next != b']', REG_ECOLLATE);
            p_b_eclass(p, cs);
            let _ = require_more(p, REG_EBRACK);
            let _ = require(p, seetwo(p, i32::from(b'='), i32::from(b']')), REG_ECOLLATE);
            if seetwo(p, i32::from(b'='), i32::from(b']')) {
                p.input.advance2();
            }
        }
        _ => {
            let start = p_b_symbol(p);
            let finish;
            if see(p, i32::from(b'-')) && p.input.more2() && p.input.peek2() != b']' {
                p.input.advance();
                if eat(p, i32::from(b'-')) {
                    finish = i32::from(b'-');
                } else {
                    finish = p_b_symbol(p);
                }
            } else {
                finish = start;
            }
            if start == finish {
                chadd(p, cs, start);
            } else if p.mb_cur_max > 1 {
                let _ = require(p, start <= finish, REG_ERANGE);
                chaddrange(p, cs, start, finish);
            } else {
                let _ = require(p, p_range_cmp(start, finish) <= 0, REG_ERANGE);
                let mut value = 0i32;
                while value <= 255 {
                    if p_range_cmp(start, value) <= 0 && p_range_cmp(value, finish) <= 0 {
                        chadd(p, cs, value);
                    }
                    value = value.saturating_add(1);
                }
            }
        }
    }
}

/// `p_b_pseudoclass` (`\w`, `\W`, `\s`, `\S` under `REG_GNU`).
fn p_b_pseudoclass(p: &mut Parse, c: i32) {
    let Some(index) = allocset(p) else {
        return;
    };
    let mut cs = take_set(p, index);
    if p.g.cflags & REG_ICASE != 0 {
        cs.icase = true;
    }
    let named: &[u8] = match c {
        x if x == i32::from(b'W') => {
            cs.invert = true;
            b"alnum"
        }
        x if x == i32::from(b'w') => b"alnum",
        x if x == i32::from(b'S') => {
            cs.invert = true;
            b"space"
        }
        x if x == i32::from(b's') => b"space",
        _ => {
            put_set(p, index, cs);
            return;
        }
    };
    p_b_cclass_named(p, &mut cs, named);
    put_set(p, index, cs);
    emit(p, OANYOF, index);
}

/// `p_b_cclass`.
fn p_b_cclass(p: &mut Parse, cs: &mut Cset) {
    let start = p.input.pos();
    while p.input.more() && p.input.peek().is_ascii_alphabetic() {
        p.input.advance();
    }
    let end = p.input.pos();
    let len = end.saturating_sub(start);
    if len >= 15 {
        let _ = seterror(p, REG_ECTYPE);
        return;
    }
    let mut name = [0u8; 16];
    for (slot, &byte) in name.iter_mut().zip(p.input.slice(start, end).iter()) {
        *slot = byte;
    }
    p_b_cclass_named(p, cs, name.get(..len).unwrap_or(&[]));
}

/// `p_b_cclass_named`.
fn p_b_cclass_named(p: &mut Parse, cs: &mut Cset, name: &[u8]) {
    match p.loc.wctype(name) {
        Some(class) => chaddtype(p, cs, class),
        None => {
            let _ = seterror(p, REG_ECTYPE);
        }
    }
}

/// `p_b_eclass`.
fn p_b_eclass(p: &mut Parse, cs: &mut Cset) {
    let c = p_b_coll_elem(p, i32::from(b'='));
    chadd(p, cs, c);
}

/// `p_b_symbol`.
fn p_b_symbol(p: &mut Parse) -> i32 {
    let _ = require_more(p, REG_EBRACK);
    if !eattwo(p, i32::from(b'['), i32::from(b'.')) {
        return wgetnext(p);
    }
    let value = p_b_coll_elem(p, i32::from(b'.'));
    let _ = require(p, seetwo(p, i32::from(b'.'), i32::from(b']')), REG_ECOLLATE);
    if seetwo(p, i32::from(b'.'), i32::from(b']')) {
        p.input.advance2();
    }
    value
}

/// `p_b_coll_elem`.
fn p_b_coll_elem(p: &mut Parse, endc: i32) -> i32 {
    let start = p.input.pos();
    while p.input.more() && !seetwo(p, endc, i32::from(b']')) {
        p.input.advance();
    }
    if !p.input.more() {
        let _ = seterror(p, REG_EBRACK);
        return 0;
    }
    let end = p.input.pos();
    let known = cname::lookup(p.input.slice(start, end));
    if let Some(code) = known {
        return i32::from(code);
    }
    let mut state = raven_libc_ctype::MbState::INITIAL;
    let decoded = p.loc.mbrtowc(p.input.slice(start, end), &mut state);
    match decoded {
        crate::locale::MbResult::Ok(wc, used) if used == end.saturating_sub(start) => wc as i32,
        crate::locale::MbResult::Ok(_, _) => {
            let _ = seterror(p, REG_ECOLLATE);
            0
        }
        crate::locale::MbResult::Incomplete | crate::locale::MbResult::Invalid => {
            let _ = seterror(p, REG_ILLSEQ);
            0
        }
    }
}

/// `may_escape`.
fn may_escape(p: &Parse, ch: i32) -> bool {
    if ch < 0 {
        return true;
    }
    if p.loc.iswalpha(ch as u32) || ch == i32::from(b'\'') || ch == i32::from(b'`') {
        return false;
    }
    true
}

/// `othercase`.
fn othercase(p: &Parse, ch: i32) -> i32 {
    if p.loc.iswupper(ch as u32) {
        p.loc.towlower(ch as u32) as i32
    } else if p.loc.iswlower(ch as u32) {
        p.loc.towupper(ch as u32) as i32
    } else {
        ch
    }
}

/// `bothcases`: parse a one-character bracket with the other case.
fn bothcases(p: &mut Parse, ch: i32) {
    let mut bracket = [0u8; 8];
    let Some(length) = encode(p, ch, &mut bracket) else {
        return;
    };
    let mut program = [0u8; 24];
    let total = length.saturating_add(1);
    if total > program.len() {
        return;
    }
    for (slot, &byte) in program.iter_mut().zip(bracket.iter()).take(length) {
        *slot = byte;
    }
    if let Some(slot) = program.get_mut(length) {
        *slot = b']';
    }
    if let Some(saved) = p.input.push_local(program.get(..total).unwrap_or(&[])) {
        p_bracket(p);
        p.input.pop_local(saved);
    }
}

/// `ordinary`.
fn ordinary(p: &mut Parse, ch: i32) {
    if p.g.cflags & REG_ICASE != 0 && ch >= 0 && p.loc.iswalpha(ch as u32) && othercase(p, ch) != ch
    {
        bothcases(p, ch);
    } else if (ch as u32) & OPDMASK == ch as u32 {
        emit(p, OCHAR, ch as usize);
    } else {
        let Some(index) = allocset(p) else {
            return;
        };
        let mut cs = take_set(p, index);
        chadd(p, &mut cs, ch);
        put_set(p, index, cs);
        emit(p, OANYOF, index);
    }
}

/// `nonnewline`: parse the synthetic `[^\n]` of `REG_NEWLINE`.
fn nonnewline(p: &mut Parse) {
    let program = *b"^\n]";
    if let Some(saved) = p.input.push_local(&program) {
        p_bracket(p);
        p.input.pop_local(saved);
    }
}

/// `repeat`.
fn repeat(p: &mut Parse, start: Sopno, from: i32, to: i32) {
    const N: i32 = 2;
    const INF: i32 = 3;
    fn map(n: i32) -> i32 {
        if n <= 1 {
            n
        } else if n == INFINITY {
            INF
        } else {
            N
        }
    }
    fn rep(f: i32, t: i32) -> i32 {
        f.saturating_mul(8).saturating_add(t)
    }

    let finish = here(p);
    if p.error != 0 {
        return;
    }
    if from > to {
        bad_assert(p);
        return;
    }
    match rep(map(from), map(to)) {
        x if x == rep(0, 0) => drop_strip(p, finish.saturating_sub(start) as usize),
        x if x == rep(0, 1) || x == rep(0, N) || x == rep(0, INF) => {
            insert(p, OCH_, start);
            repeat(p, start.saturating_add(1), 1, to);
            astern(p, OOR1, start);
            ahead(p, start);
            emit(p, OOR2, 0);
            ahead(p, here(p).saturating_sub(1));
            astern(p, O_CH, here(p).saturating_sub(2));
        }
        x if x == rep(1, 1) => {}
        x if x == rep(1, N) => {
            insert(p, OCH_, start);
            astern(p, OOR1, start);
            ahead(p, start);
            emit(p, OOR2, 0);
            ahead(p, here(p).saturating_sub(1));
            astern(p, O_CH, here(p).saturating_sub(2));
            let copy = dupl(p, start.saturating_add(1), finish.saturating_add(1));
            repeat(p, copy, 1, to.saturating_sub(1));
        }
        x if x == rep(1, INF) => {
            insert(p, OPLUS_, start);
            astern(p, O_PLUS, start);
        }
        x if x == rep(N, N) => {
            let copy = dupl(p, start, finish);
            repeat(p, copy, from.saturating_sub(1), to.saturating_sub(1));
        }
        x if x == rep(N, INF) => {
            let copy = dupl(p, start, finish);
            repeat(p, copy, from.saturating_sub(1), to);
        }
        _ => {
            let _ = seterror(p, REG_ASSERT);
        }
    }
}

/// `stripsnug`: the strip is already tight; record its length.
fn stripsnug(p: &mut Parse) {
    p.g.nstates = p.strip.len() as Sopno;
}

/// End one `findmust` sequence, exactly as the three donor case groups do.
fn must_close(
    g: &mut ReGuts,
    newlen: u32,
    offset: &mut i32,
    start: &mut Option<usize>,
    newstart: Option<usize>,
) {
    if newlen > g.mlen as u32 {
        *start = newstart;
        g.mlen = newlen as usize;
        if *offset > -1 {
            g.moffset = g.moffset.saturating_add(*offset);
            *offset = newlen as i32;
        } else {
            g.moffset = *offset;
        }
    } else if *offset > -1 {
        *offset = offset.saturating_add(newlen as i32);
    }
}

/// `altoffset`: the largest offset among the choices.
fn altoffset(strip: &[Sop], mut scan: usize, offset: i32) -> i32 {
    if offset == -1 {
        return -1;
    }
    let mut largest = 0i32;
    let mut attempt = 0i32;
    let mut word = strip.get(scan).copied().unwrap_or(OEND);
    scan = scan.saturating_add(1);
    while op(word) != O_QUEST && op(word) != O_CH {
        match op(word) {
            x if x == OOR1 => {
                if attempt > largest {
                    largest = attempt;
                }
                attempt = 0;
            }
            x if x == OQUEST_ || x == OCH_ => {
                attempt = altoffset(strip, scan, attempt);
                if attempt == -1 {
                    return -1;
                }
                scan = scan.saturating_sub(1);
                let mut inner = word;
                loop {
                    scan = scan.saturating_add(opnd(inner));
                    inner = strip.get(scan).copied().unwrap_or(OEND);
                    let kind = op(inner);
                    if kind != O_QUEST && kind != O_CH && kind != OOR2 {
                        return -1;
                    }
                    if kind == O_QUEST || kind == O_CH {
                        break;
                    }
                }
                scan = scan.saturating_add(1);
            }
            x if x == OANYOF || x == OCHAR || x == OANY => {
                attempt = attempt.saturating_add(1);
            }
            x if x == OWBND
                || x == crate::regex2::OEOW
                || x == crate::regex2::OBOW
                || x == crate::regex2::ONWBND
                || x == OLPAREN
                || x == ORPAREN
                || x == OOR2 => {}
            _ => {
                attempt = -1;
            }
        }
        if attempt == -1 {
            return -1;
        }
        word = strip.get(scan).copied().unwrap_or(OEND);
        scan = scan.saturating_add(1);
    }
    if attempt > largest {
        largest = attempt;
    }
    largest.saturating_add(offset)
}

/// `findmust`: the longest mandatory literal string.
fn findmust(p: &mut Parse) {
    if p.error != 0 {
        return;
    }
    let mut scan = 1usize;
    let mut start: Option<usize> = None;
    let mut newstart: Option<usize> = None;
    let mut newlen = 0u32;
    let mut offset = 0i32;
    p.g.moffset = 0;
    loop {
        let word = p.strip.get(scan).copied().unwrap_or(OEND);
        scan = scan.saturating_add(1);
        let kind = op(word);
        let mut too_hard = false;
        match kind {
            x if x == OCHAR => {
                if newlen == 0 {
                    newstart = scan.checked_sub(1);
                }
                let mut buffer = [0u8; 8];
                match encode(p, opnd(word) as i32, &mut buffer) {
                    Some(length) => newlen = newlen.saturating_add(length as u32),
                    None => too_hard = true,
                }
            }
            x if x == OPLUS_ || x == OLPAREN || x == ORPAREN => {}
            x if x == OQUEST_ || x == OCH_ => {
                offset = altoffset(&p.strip, scan, offset);
                scan = scan.saturating_sub(1);
                let mut inner = word;
                loop {
                    scan = scan.saturating_add(opnd(inner));
                    inner = p.strip.get(scan).copied().unwrap_or(OEND);
                    let inner_kind = op(inner);
                    if inner_kind != O_QUEST && inner_kind != O_CH && inner_kind != OOR2 {
                        p.g.iflags |= crate::consts::BAD;
                        return;
                    }
                    if inner_kind == O_QUEST || inner_kind == O_CH {
                        break;
                    }
                }
                must_close(&mut p.g, newlen, &mut offset, &mut start, newstart);
                newlen = 0;
            }
            x if x == crate::regex2::OBOW
                || x == crate::regex2::OEOW
                || x == OBOL
                || x == OEOL
                || x == OBOS
                || x == OEOS
                || x == OWBND
                || x == crate::regex2::ONWBND
                || x == O_QUEST
                || x == O_CH
                || x == OEND =>
            {
                must_close(&mut p.g, newlen, &mut offset, &mut start, newstart);
                newlen = 0;
            }
            x if x == OANY || x == OANYOF => {
                must_close(&mut p.g, newlen, &mut offset, &mut start, newstart);
                if offset > -1 {
                    offset = offset.saturating_add(1);
                }
                newlen = 0;
            }
            _ => {
                too_hard = true;
            }
        }
        if too_hard {
            must_close(&mut p.g, newlen, &mut offset, &mut start, newstart);
            offset = -1;
            newlen = 0;
        }
        if op(word) == OEND {
            break;
        }
    }

    if p.g.mlen == 0 {
        p.g.moffset = -1;
        return;
    }

    let Some(start) = start else {
        p.g.mlen = 0;
        p.g.moffset = -1;
        return;
    };
    let mlen = p.g.mlen;
    let mut must = Vec::new();
    if must.try_reserve(mlen).is_err() {
        p.g.mlen = 0;
        p.g.moffset = -1;
        return;
    }
    let mut scan = start;
    while must.len() < mlen {
        let mut word;
        loop {
            word = p.strip.get(scan).copied().unwrap_or(OEND);
            scan = scan.saturating_add(1);
            if op(word) == OCHAR || op(word) == OEND {
                break;
            }
        }
        if op(word) == OEND {
            break;
        }
        let mut buffer = [0u8; 8];
        if let Some(length) = encode(p, opnd(word) as i32, &mut buffer) {
            for &byte in buffer.iter().take(length) {
                must.push(byte);
            }
        } else {
            break;
        }
    }
    p.g.must = Some(must);
}

/// `computejumps`.
fn computejumps(p: &mut Parse) {
    if p.error != 0 {
        return;
    }
    let mlen = p.g.mlen;
    let Some(must) = p.g.must.clone() else {
        return;
    };
    let mut charjump = alloc::vec![mlen; 256];
    for (index, &byte) in must.iter().enumerate() {
        let value = mlen.saturating_sub(index).saturating_sub(1);
        if let Some(slot) = charjump.get_mut(usize::from(byte)) {
            *slot = value;
        }
    }
    p.g.charjump = Some(charjump);
}

/// `computematchjumps`.
fn computematchjumps(p: &mut Parse) {
    if p.error != 0 {
        return;
    }
    let mlen = p.g.mlen;
    let Some(must) = p.g.must.clone() else {
        return;
    };
    if mlen == 0 {
        return;
    }
    let mut pmatches = alloc::vec![0usize; mlen];
    let mut matchjump = alloc::vec![0usize; mlen];
    for (index, slot) in matchjump.iter_mut().enumerate() {
        *slot = mlen
            .saturating_mul(2)
            .saturating_sub(index)
            .saturating_sub(1);
    }

    let mut suffix = mlen;
    let mut mindex = mlen;
    loop {
        if mindex == 0 {
            break;
        }
        mindex = mindex.saturating_sub(1);
        if let Some(slot) = pmatches.get_mut(mindex) {
            *slot = suffix;
        }
        while suffix < mlen && must.get(mindex).copied() != must.get(suffix).copied() {
            let candidate = mlen.saturating_sub(mindex).saturating_sub(1);
            if let Some(slot) = matchjump.get_mut(suffix) {
                *slot = (*slot).min(candidate);
            }
            suffix = pmatches.get(suffix).copied().unwrap_or(0);
        }
        suffix = suffix.saturating_sub(1);
    }

    let mut index = 0usize;
    while index <= suffix {
        let candidate = mlen.saturating_add(suffix).saturating_sub(index);
        if let Some(slot) = matchjump.get_mut(index) {
            *slot = (*slot).min(candidate);
        }
        index = index.wrapping_add(1);
    }

    if let Some(&first) = pmatches.get(suffix) {
        let mut ssuffix = first;
        let mut at = suffix;
        while at < mlen {
            while at <= ssuffix && at < mlen {
                let candidate = mlen.saturating_add(ssuffix).saturating_sub(at);
                if let Some(slot) = matchjump.get_mut(at) {
                    *slot = (*slot).min(candidate);
                }
                at = at.wrapping_add(1);
            }
            if at < mlen {
                ssuffix = pmatches.get(ssuffix).copied().unwrap_or(0);
            }
        }
    }

    p.g.matchjump = Some(matchjump);
}

/// `pluscount`: how deeply `+` nests.
fn pluscount(p: &mut Parse) -> Sopno {
    if p.error != 0 {
        return 0;
    }
    let mut scan = 1usize;
    let mut plusnest: Sopno = 0;
    let mut maxnest: Sopno = 0;
    loop {
        let word = p.strip.get(scan).copied().unwrap_or(OEND);
        scan = scan.saturating_add(1);
        match op(word) {
            x if x == OPLUS_ => plusnest = plusnest.saturating_add(1),
            x if x == O_PLUS => {
                if plusnest > maxnest {
                    maxnest = plusnest;
                }
                plusnest = plusnest.saturating_sub(1);
            }
            _ => {}
        }
        if op(word) == OEND {
            break;
        }
    }
    if plusnest != 0 {
        p.g.iflags |= crate::consts::BAD;
    }
    maxnest
}

/// `regcomp`: compile `pattern` under `cflags` with the calling thread's locale.
///
/// `REG_PEND` is expressed by the slice length; `REG_DUMP` is masked off. The returned
/// [`Regex`] owns the same invariants as the donor's `regex_t`: [`Regex::magic_ok`] is true
/// until the value is dropped.
///
/// # Errors
///
/// One of the `REG_*` codes: the pattern errors of the donor, `REG_INVARG` for the
/// `REG_EXTENDED|REG_NOSPEC` conflict, and `REG_ESPACE` when one of the hardening limits or
/// an allocation is hit.
pub fn regcomp(pattern: &[u8], cflags: u32, loc: &dyn RegexLocale) -> Result<Regex, RegError> {
    let cflags = cflags & !crate::consts::REG_DUMP;
    if cflags & REG_EXTENDED != 0 && cflags & REG_NOSPEC != 0 {
        return Err(RegError(REG_INVARG));
    }

    let len = pattern.len();
    const _: () = assert!(core::mem::size_of::<Sop>() == 4);
    let maxlen = (usize::MAX >> 1)
        .wrapping_div(4)
        .wrapping_mul(2)
        .wrapping_div(3);
    if len >= maxlen {
        return Err(RegError(REG_ESPACE));
    }
    let needed = len
        .checked_mul(3)
        .and_then(|value| value.checked_div(2))
        .and_then(|value| value.checked_add(1))
        .ok_or(RegError(REG_ESPACE))?;
    let ssize = needed.clamp(1, MAX_STRIP);

    let bre = cflags & REG_EXTENDED == 0;
    let (gnuext, allowbranch) = if cflags & REG_GNU != 0 {
        (true, true)
    } else {
        (false, !bre)
    };
    let mb_cur_max = loc.mb_cur_max();
    let mut parse = Parse {
        input: Input::new(pattern),
        error: 0,
        gnuext,
        strip: Vec::new(),
        ssize,
        g: ReGuts {
            cflags,
            ..ReGuts::default()
        },
        pbegin: [0; NPAREN],
        pend: [0; NPAREN],
        allowbranch,
        bre,
        depth: 0,
        loc,
        nc: nc(mb_cur_max),
        mb_cur_max,
    };

    emit(&mut parse, OEND, 0);
    parse.g.firststate = here(&parse).saturating_sub(1);
    if cflags & REG_NOSPEC != 0 {
        p_str(&mut parse);
    } else {
        p_re(&mut parse, OUT, OUT);
        let _ = parse.gnuext;
    }
    emit(&mut parse, OEND, 0);
    parse.g.laststate = here(&parse).saturating_sub(1);

    stripsnug(&mut parse);
    findmust(&mut parse);
    if parse.g.mlen > 3 {
        computejumps(&mut parse);
        computematchjumps(&mut parse);
        if parse.g.matchjump.is_none() && parse.g.charjump.is_some() {
            parse.g.charjump = None;
        }
    }
    parse.g.nplus = pluscount(&mut parse);

    if parse.error != 0 {
        return Err(RegError(parse.error));
    }
    if parse.g.iflags & crate::consts::BAD != 0 {
        return Err(RegError(REG_ASSERT));
    }
    parse.g.magic = MAGIC2;
    parse.g.strip = core::mem::take(&mut parse.strip);
    let guts = parse.g;
    Ok(Regex {
        magic1: MAGIC1,
        guts,
    })
}

#[cfg(test)]
#[path = "regcomp_ut.rs"]
mod tests;
