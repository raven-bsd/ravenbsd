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
//	@(#)engine.c	8.5 (Berkeley) 3/20/94
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/regex/engine.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// Altered: transpiled to Rust; hardening limits MAX_STRIP, BACKREF_BUDGET, MAX_PAREN_DEPTH, BACKREF_DEPTH_MAX; generic state-set engine; wide-character access through RegexLocale.

//! The matching engine (`engine.c`).
//!
//! The donor text is `#include`d three times with different state-set macros; this module is
//! one generic engine over [`StateSet`] plus the `const MULTIBYTE` decoder switch (A-rx-1,
//! A-rx-4). The names `stepback`, `matcher`, `dissect`, `backref`, `walk` and `step` are kept
//! so the port can be diffed against the C. Tracing (`NOTE`, `SP`, `AT`, `REG_TRACE`) is
//! deleted.
//!
//! Hardening H-RE-2 is in `backref` alone: a per-`regexec` budget and a nesting-depth limit
//! turn a runaway backtracking pattern into [`crate::consts::REG_ESPACE`]. The linear paths
//! (`dissect`, `walk`, `step`) never call it.

use alloc::vec::Vec;

use raven_libc_ctype::MbState;

use crate::consts::{BACKREF_DEPTH_MAX, MAX_RECURSION, REG_ESPACE, REG_NOMATCH};
use crate::locale::{MbResult, RegexLocale};
use crate::regex2::{
    O_BACK, O_CH, O_PLUS, O_QUEST, OANY, OANYOF, OBACK_, OBOL, OBOS, OCH_, OCHAR, OEND, OEOL, OEOS,
    OLPAREN, OOR1, OOR2, OPLUS_, OQUEST_, ORPAREN, OWBND, ReGuts, Sopno, isword, op, opnd, sop,
};
use crate::utils::OUT;
use crate::{RegError, RegMatch};

/// `BOL`: an artificial beginning-of-line character.
const BOL: i32 = OUT - 1;
/// `EOL`: an artificial end-of-line character.
const EOL: i32 = BOL - 1;
/// `BOLEOL`: both anchors at once.
const BOLEOL: i32 = BOL - 2;
/// `NOTHING`: the no-character step.
const NOTHING: i32 = BOL - 3;
/// `BOW`: beginning of a word.
const BOW: i32 = BOL - 4;
/// `EOW`: end of a word.
const EOW: i32 = BOL - 5;
/// `BADCHAR`: the dummy used for an undecodable byte in the multibyte matcher.
const BADCHAR: i32 = BOL - 6;
/// `NWBND`: not a word boundary.
const NWBND: i32 = BOL - 7;

/// `SBOS`: the start of the subject is here.
const SBOS: i32 = 0x0001;
/// `SEOS`: the end of the subject is here.
const SEOS: i32 = 0x0002;

/// `NONCHAR(c)`: a sentinel, not a decoded character.
#[must_use]
const fn nonchar(c: i32) -> bool {
    c <= OUT
}

/// A set of `sopno` states.
pub trait StateSet: Clone {
    /// An empty set of `nstates` states.
    fn new(nstates: usize) -> Self;
    /// `CLEAR`.
    fn clear(&mut self);
    /// `SET1`.
    fn set1(&mut self, index: usize);
    /// `ISSET`.
    fn is_set(&self, index: usize) -> bool;
    /// `ASSIGN`.
    fn assign(&mut self, source: &Self);
    /// `EQ`.
    fn eq(&self, other: &Self) -> bool;
    /// `FWD` with a distinct source set.
    fn fwd_from(&mut self, source: &Self, here: usize, distance: usize);
    /// `FWD` with the destination as its own source.
    fn fwd_self(&mut self, here: usize, distance: usize);
    /// `BACK` with the destination as its own source.
    fn back_self(&mut self, here: usize, distance: usize);
    /// `ISSETBACK`.
    fn issetback(&self, here: usize, distance: usize) -> bool;
    /// `ISSTATEIN`.
    fn is_state_in(&self, here: usize) -> bool;
    /// `EQ(st, empty)`.
    fn is_empty(&self) -> bool;
}

/// The `smatcher` state representation: one `u64` (A-rx-2).
#[derive(Clone, Default)]
pub struct SmallSet(u64);

impl StateSet for SmallSet {
    fn new(_nstates: usize) -> Self {
        Self(0)
    }

    fn clear(&mut self) {
        self.0 = 0;
    }

    fn set1(&mut self, index: usize) {
        if index < 64 {
            self.0 |= 1u64 << index;
        }
    }

    fn is_set(&self, index: usize) -> bool {
        index < 64 && self.0 & (1u64 << index) != 0
    }

    fn assign(&mut self, source: &Self) {
        self.0 = source.0;
    }

    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }

    fn fwd_from(&mut self, source: &Self, here: usize, distance: usize) {
        if here >= 64 || distance >= 64 {
            return;
        }
        let bit = source.0 & (1u64 << here);
        self.0 |= bit.wrapping_shl(distance as u32);
    }

    fn fwd_self(&mut self, here: usize, distance: usize) {
        if here >= 64 || distance >= 64 {
            return;
        }
        let bit = self.0 & (1u64 << here);
        self.0 |= bit.wrapping_shl(distance as u32);
    }

    fn back_self(&mut self, here: usize, distance: usize) {
        if here >= 64 || distance > here {
            return;
        }
        let bit = self.0 & (1u64 << here);
        self.0 |= bit.wrapping_shr(distance as u32);
    }

    fn issetback(&self, here: usize, distance: usize) -> bool {
        here >= distance && self.0 & (1u64 << here.wrapping_sub(distance)) != 0
    }

    fn is_state_in(&self, here: usize) -> bool {
        here < 64 && self.0 & (1u64 << here) != 0
    }

    fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

/// The `lmatcher`/`mmatcher` state representation: one byte per state.
#[derive(Clone, Default)]
pub struct LargeSet(Vec<u8>);

impl StateSet for LargeSet {
    fn new(nstates: usize) -> Self {
        Self(alloc::vec![0; nstates])
    }

    fn clear(&mut self) {
        self.0.fill(0);
    }

    fn set1(&mut self, index: usize) {
        if let Some(slot) = self.0.get_mut(index) {
            *slot = 1;
        }
    }

    fn is_set(&self, index: usize) -> bool {
        self.0.get(index).copied().unwrap_or(0) != 0
    }

    fn assign(&mut self, source: &Self) {
        if self.0.len() == source.0.len() {
            self.0.copy_from_slice(&source.0);
        } else {
            self.0 = source.0.clone();
        }
    }

    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }

    fn fwd_from(&mut self, source: &Self, here: usize, distance: usize) {
        if source.0.get(here).copied().unwrap_or(0) == 0 {
            return;
        }
        if let Some(target) = here.checked_add(distance)
            && let Some(slot) = self.0.get_mut(target)
        {
            *slot = 1;
        }
    }

    fn fwd_self(&mut self, here: usize, distance: usize) {
        if self.0.get(here).copied().unwrap_or(0) == 0 {
            return;
        }
        if let Some(target) = here.checked_add(distance)
            && let Some(slot) = self.0.get_mut(target)
        {
            *slot = 1;
        }
    }

    fn back_self(&mut self, here: usize, distance: usize) {
        if self.0.get(here).copied().unwrap_or(0) == 0 {
            return;
        }
        if let Some(target) = here.checked_sub(distance)
            && let Some(slot) = self.0.get_mut(target)
        {
            *slot = 1;
        }
    }

    fn issetback(&self, here: usize, distance: usize) -> bool {
        here.checked_sub(distance)
            .and_then(|index| self.0.get(index))
            .copied()
            .unwrap_or(0)
            != 0
    }

    fn is_state_in(&self, here: usize) -> bool {
        self.0.get(here).copied().unwrap_or(0) != 0
    }

    fn is_empty(&self) -> bool {
        self.0.iter().all(|&byte| byte == 0)
    }
}

/// The per-call match state (`struct match`), with the H-RE-2 fields.
struct MatchState<'a, S> {
    /// The compiled program.
    g: &'a ReGuts,
    /// The subject bytes.
    subject: &'a [u8],
    /// `eflags`, already masked.
    eflags: u32,
    /// Sub-match output, `[nsub+1]` with slot 0 unused.
    pmatch: Option<Vec<RegMatch>>,
    /// Start of the subject window.
    beginp: usize,
    /// End of the subject window.
    endp: usize,
    /// Earliest possible match start.
    coldp: usize,
    /// `[nplus+1]` saved positions for `backref`.
    lastpos: Vec<usize>,
    /// Current states.
    st: S,
    /// States for a fresh start.
    fresh: S,
    /// Temporary states.
    tmp: S,
    /// The empty set.
    empty: S,
    /// Multibyte conversion state.
    mbs: MbState,
    /// The locale seam.
    loc: &'a dyn RegexLocale,
    /// `NC` of the execution locale.
    nc: usize,
    /// H-RE-2: `backref` calls left.
    budget: u64,
    /// H-RE-2: current `backref` nesting.
    depth: u32,
    /// H-RE-2: first hardening error, `0` when none.
    error: i32,
}

/// `stepback`: move `nchar` characters back from `cur`, not past `start`.
fn stepback(
    loc: &dyn RegexLocale,
    subject: &[u8],
    start: usize,
    cur: usize,
    nchar: i32,
) -> Option<usize> {
    if loc.mb_cur_max() == 1 {
        let target = cur.checked_sub(usize::try_from(nchar).ok()?)?;
        return if target > start { Some(target) } else { None };
    }
    let mut ret = cur;
    let count = usize::try_from(nchar).ok()?;
    for _ in 0..count {
        let max = loc.mb_cur_max();
        let mut found = None;
        for mbc in 1..=max {
            let at = ret.checked_sub(mbc)?;
            if at < start {
                return None;
            }
            let mut state = MbState::INITIAL;
            if let MbResult::Ok(_, _) = loc.mbrtowc(subject.get(at..ret).unwrap_or(&[]), &mut state)
            {
                found = Some(mbc);
                break;
            }
        }
        let mbc = found?;
        ret = ret.checked_sub(mbc)?;
    }
    Some(ret)
}

/// `xmbrtowc`: decode one subject character, consuming one byte on an invalid sequence.
fn xmbrtowc<const MULTIBYTE: bool>(
    loc: &dyn RegexLocale,
    state: &mut MbState,
    input: &[u8],
    dummy: i32,
) -> (i32, usize) {
    if !MULTIBYTE {
        return match input.first() {
            Some(&byte) => (i32::from(byte), 1),
            None => (dummy, 1),
        };
    }
    match loc.mbrtowc(input, state) {
        MbResult::Ok(wc, used) => {
            let consumed = if used == 0 { 1 } else { used };
            (wc as i32, consumed)
        }
        MbResult::Incomplete | MbResult::Invalid => {
            *state = MbState::INITIAL;
            (dummy, 1)
        }
    }
}

/// `matcher`: the actual matching engine, generic over the state representation.
#[allow(clippy::too_many_lines)]
pub(crate) fn matcher<const MULTIBYTE: bool, S: StateSet>(
    g: &ReGuts,
    subject: &[u8],
    pmatch: &mut [RegMatch],
    eflags: u32,
    loc: &dyn RegexLocale,
    budget: u64,
) -> Result<(), RegError> {
    let mut nmatch = pmatch.len();
    if g.cflags & crate::consts::REG_NOSUB != 0 {
        nmatch = 0;
    }
    let mut start = 0usize;
    let mut stop = subject.len();
    if eflags & crate::consts::REG_STARTEND != 0 {
        let Some(first) = pmatch.first().copied() else {
            return Err(RegError(crate::consts::REG_INVARG));
        };
        if first.rm_so < 0 || first.rm_eo < 0 {
            return Err(RegError(crate::consts::REG_INVARG));
        }
        start = first.rm_so as usize;
        stop = first.rm_eo as usize;
        if stop < start || stop > subject.len() {
            return Err(RegError(crate::consts::REG_INVARG));
        }
    }

    let mut dp: Option<usize> = None;
    if let Some(must) = &g.must {
        let mlen = g.mlen;
        if let (Some(charjump), Some(matchjump)) = (&g.charjump, &g.matchjump) {
            if mlen == 0 {
                return Err(RegError(REG_NOMATCH));
            }
            let mustfirst = 0usize;
            let mustlast = mlen.saturating_sub(1);
            let mut pp = mustlast as isize;
            let mut at = (start as isize)
                .saturating_add(mlen as isize)
                .saturating_sub(1);
            'search: while at < stop as isize {
                while at < stop as isize {
                    let byte = subject.get(at as usize).copied().unwrap_or(0);
                    let jump = charjump.get(usize::from(byte)).copied().unwrap_or(0);
                    if jump == 0 {
                        break;
                    }
                    at = at.saturating_add(jump as isize);
                }
                if at >= stop as isize {
                    break;
                }
                loop {
                    at = at.saturating_sub(1);
                    pp = pp.saturating_sub(1);
                    if at < 0 || pp < 0 {
                        break;
                    }
                    let text = subject.get(at as usize).copied().unwrap_or(0);
                    let want = must.get(pp as usize).copied().unwrap_or(0);
                    if text != want || pp == mustfirst as isize {
                        break;
                    }
                }
                if at >= 0
                    && pp >= 0
                    && subject.get(at as usize).copied() == must.get(pp as usize).copied()
                {
                    dp = Some(at as usize);
                    break 'search;
                }
                let mjump = matchjump.get(pp.max(0) as usize).copied().unwrap_or(0);
                let byte = subject.get(at.max(0) as usize).copied().unwrap_or(0);
                let cjump = charjump.get(usize::from(byte)).copied().unwrap_or(0);
                at = at.saturating_add(cjump.max(mjump) as isize);
                pp = mustlast as isize;
            }
            if pp != mustfirst as isize {
                return Err(RegError(REG_NOMATCH));
            }
        } else {
            if mlen == 0 {
                return Err(RegError(REG_NOMATCH));
            }
            let first = must.first().copied().unwrap_or(0);
            let mut at = start;
            while at < stop {
                let enough = stop.saturating_sub(at) >= mlen;
                if subject.get(at).copied() == Some(first)
                    && enough
                    && subject.get(at..at.saturating_add(mlen)) == Some(must.as_slice())
                {
                    break;
                }
                at = at.saturating_add(1);
            }
            if at == stop {
                return Err(RegError(REG_NOMATCH));
            }
            dp = Some(at);
        }
    }

    let nstates = g.nstates as usize;
    let mut m: MatchState<'_, S> = MatchState {
        g,
        subject,
        eflags,
        pmatch: None,
        beginp: start,
        endp: stop,
        coldp: start,
        lastpos: Vec::new(),
        st: S::new(nstates),
        fresh: S::new(nstates),
        tmp: S::new(nstates),
        empty: S::new(nstates),
        mbs: MbState::INITIAL,
        loc,
        nc: crate::utils::nc(loc.mb_cur_max()),
        budget,
        depth: 0,
        error: 0,
    };
    m.empty.clear();

    if let Some(found) = dp
        && g.moffset > -1
        && let Some(nstart) = stepback(loc, subject, start, found, g.moffset)
    {
        start = nstart;
    }

    let gf = g.firststate.saturating_add(1);
    let gl = g.laststate;
    let mut endp = start;
    let mut error = 0i32;
    'outer: loop {
        match walk::<MULTIBYTE, S>(&mut m, start, stop, gf, gl, true) {
            Some(value) => endp = value,
            None => {
                error = REG_NOMATCH;
                break 'outer;
            }
        }
        if nmatch == 0 && !g.backrefs {
            break 'outer;
        }

        loop {
            let coldp = m.coldp;
            match walk::<MULTIBYTE, S>(&mut m, coldp, stop, gf, gl, false) {
                Some(value) => {
                    endp = value;
                    break;
                }
                None => {
                    if m.coldp >= m.endp {
                        error = REG_NOMATCH;
                        break 'outer;
                    }
                    let (_, used) = xmbrtowc::<MULTIBYTE>(
                        loc,
                        &mut m.mbs,
                        subject.get(m.coldp..m.endp).unwrap_or(&[]),
                        0,
                    );
                    m.coldp = m.coldp.saturating_add(used);
                }
            }
        }
        if nmatch == 1 && !g.backrefs {
            break 'outer;
        }

        let nsub = g.nsub;
        if m.pmatch.is_none() {
            let mut table = Vec::new();
            if table.try_reserve(nsub.saturating_add(1)).is_err() {
                error = REG_ESPACE;
                break 'outer;
            }
            table.resize(
                nsub.saturating_add(1),
                RegMatch {
                    rm_so: -1,
                    rm_eo: -1,
                },
            );
            m.pmatch = Some(table);
        }
        if let Some(table) = m.pmatch.as_mut() {
            for index in 1..=nsub {
                if let Some(slot) = table.get_mut(index) {
                    slot.rm_so = -1;
                    slot.rm_eo = -1;
                }
            }
        }
        let mut dp_here: Option<usize>;
        if !g.backrefs && m.eflags & crate::consts::REG_BACKR == 0 {
            let coldp = m.coldp;
            dp_here = dissect::<MULTIBYTE, S>(&mut m, coldp, endp, gf, gl);
        } else {
            if g.nplus > 0 && m.lastpos.is_empty() {
                let mut positions = Vec::new();
                if positions
                    .try_reserve((g.nplus as usize).saturating_add(1))
                    .is_err()
                {
                    error = REG_ESPACE;
                    break 'outer;
                }
                positions.resize((g.nplus as usize).saturating_add(1), 0);
                m.lastpos = positions;
            }
            let coldp = m.coldp;
            dp_here = backref::<MULTIBYTE, S>(&mut m, coldp, endp, gf, gl, 0, 0);
            if m.error != 0 {
                error = m.error;
                break 'outer;
            }
        }
        if dp_here.is_some() {
            break 'outer;
        }

        loop {
            if dp_here.is_some() || endp <= m.coldp {
                break;
            }
            let coldp = m.coldp;
            let Some(shorter) =
                walk::<MULTIBYTE, S>(&mut m, coldp, endp.saturating_sub(1), gf, gl, false)
            else {
                break;
            };
            endp = shorter;
            dp_here = backref::<MULTIBYTE, S>(&mut m, coldp, endp, gf, gl, 0, 0);
            if m.error != 0 {
                error = m.error;
                break 'outer;
            }
        }
        if dp_here.is_some() {
            break 'outer;
        }

        let (_, used) = xmbrtowc::<MULTIBYTE>(
            loc,
            &mut m.mbs,
            subject.get(m.coldp..stop).unwrap_or(&[]),
            0,
        );
        start = m.coldp.saturating_add(used);
        if start > stop {
            error = REG_NOMATCH;
            break 'outer;
        }
    }

    if error != 0 {
        return Err(RegError(error));
    }

    if nmatch > 0 {
        let first = RegMatch {
            rm_so: m.coldp as i64,
            rm_eo: endp as i64,
        };
        if let Some(slot) = pmatch.first_mut() {
            *slot = first;
        }
    }
    if nmatch > 1 {
        for index in 1..nmatch {
            let value = if index <= g.nsub {
                m.pmatch
                    .as_ref()
                    .and_then(|table| table.get(index))
                    .copied()
                    .unwrap_or(RegMatch {
                        rm_so: -1,
                        rm_eo: -1,
                    })
            } else {
                RegMatch {
                    rm_so: -1,
                    rm_eo: -1,
                }
            };
            if let Some(slot) = pmatch.get_mut(index) {
                *slot = value;
            }
        }
    }
    Ok(())
}

/// `ISBOW`.
fn is_bow<S>(m: &MatchState<'_, S>, at: usize) -> bool {
    let subject = m.subject;
    at < m.endp
        && isword(i32::from(subject.get(at).copied().unwrap_or(0)), m.loc)
        && ((at == m.beginp && m.eflags & crate::consts::REG_NOTBOL == 0)
            || (at > 0
                && !isword(
                    i32::from(subject.get(at.saturating_sub(1)).copied().unwrap_or(0)),
                    m.loc,
                )))
}

/// `ISEOW`.
fn is_eow<S>(m: &MatchState<'_, S>, at: usize) -> bool {
    let subject = m.subject;
    ((at == m.endp && m.eflags & crate::consts::REG_NOTEOL == 0)
        || (at < m.endp
            && subject.get(at).copied() == Some(b'\n')
            && m.g.cflags & crate::consts::REG_NEWLINE != 0)
        || (at < m.endp && !isword(i32::from(subject.get(at).copied().unwrap_or(0)), m.loc)))
        && (at > m.beginp
            && isword(
                i32::from(subject.get(at.saturating_sub(1)).copied().unwrap_or(0)),
                m.loc,
            ))
}

/// `dissect`: figure out what matched what, without back-references.
#[allow(clippy::too_many_lines)]
fn dissect<const MULTIBYTE: bool, S: StateSet>(
    m: &mut MatchState<'_, S>,
    start: usize,
    stop: usize,
    startst: Sopno,
    stopst: Sopno,
) -> Option<usize> {
    let mut sp = start;
    let mut ss = startst;
    while ss < stopst {
        let mut es = ss;
        match op(m.g.word(es as usize)) {
            x if x == OPLUS_ || x == OQUEST_ => {
                es = es.saturating_add(opnd(m.g.word(es as usize)) as Sopno);
            }
            x if x == OCH_ => loop {
                if es as usize >= m.g.strip.len() {
                    return None;
                }
                if op(m.g.word(es as usize)) == O_CH {
                    break;
                }
                es = es.saturating_add(opnd(m.g.word(es as usize)) as Sopno);
            },
            _ => {}
        }
        es = es.saturating_add(1);
        let operator = op(m.g.word(ss as usize));
        if operator == OCHAR || operator == OANY || operator == OANYOF {
            let (_, used) =
                xmbrtowc::<MULTIBYTE>(m.loc, &mut m.mbs, m.subject.get(sp..stop).unwrap_or(&[]), 0);
            sp = sp.saturating_add(used);
        } else if operator == OBOL
            || operator == OEOL
            || operator == OWBND
            || operator == crate::regex2::OEOW
            || operator == OBOS
            || operator == OEOS
            || operator == crate::regex2::ONWBND
        {
        } else if operator == OLPAREN {
            let index = opnd(m.g.word(ss as usize));
            if let Some(table) = m.pmatch.as_mut()
                && let Some(slot) = table.get_mut(index)
            {
                slot.rm_so = sp as i64;
            }
        } else if operator == ORPAREN {
            let index = opnd(m.g.word(ss as usize));
            if let Some(table) = m.pmatch.as_mut()
                && let Some(slot) = table.get_mut(index)
            {
                slot.rm_eo = sp as i64;
            }
        } else if operator == OQUEST_ || operator == OPLUS_ || operator == OCH_ {
            let mut stp = stop;
            let rest;
            loop {
                let candidate = walk::<MULTIBYTE, S>(m, sp, stp, ss, es, false)?;
                let tail = walk::<MULTIBYTE, S>(m, candidate, stop, es, stopst, false);
                if tail == Some(stop) {
                    rest = candidate;
                    break;
                }
                if candidate == 0 {
                    return None;
                }
                stp = candidate.saturating_sub(1);
                if stp < sp {
                    return None;
                }
            }
            let ssub = ss.saturating_add(1);
            let esub = es.saturating_sub(1);
            if operator == OQUEST_ {
                if walk::<MULTIBYTE, S>(m, sp, rest, ssub, esub, false).is_some() {
                    let _ = dissect::<MULTIBYTE, S>(m, sp, rest, ssub, esub);
                }
            } else if operator == OPLUS_ {
                let mut ssp = sp;
                let mut oldssp = ssp;
                let mut sep: Option<usize> = None;
                loop {
                    let found = walk::<MULTIBYTE, S>(m, ssp, rest, ssub, esub, false);
                    match found {
                        None => break,
                        Some(value) if value == ssp => {
                            sep = Some(value);
                            break;
                        }
                        Some(value) => {
                            oldssp = ssp;
                            ssp = value;
                        }
                    }
                }
                let (sep, ssp) = match sep {
                    Some(value) => (value, ssp),
                    None => (ssp, oldssp),
                };
                let _ = walk::<MULTIBYTE, S>(m, ssp, sep, ssub, esub, false);
                let _ = dissect::<MULTIBYTE, S>(m, ssp, sep, ssub, esub);
            } else {
                let mut ssub = ss.saturating_add(1);
                let mut esub = ss
                    .saturating_add(opnd(m.g.word(ss as usize)) as Sopno)
                    .saturating_sub(1);
                loop {
                    if walk::<MULTIBYTE, S>(m, sp, rest, ssub, esub, false) == Some(rest) {
                        break;
                    }
                    if op(m.g.word(esub as usize)) == O_CH {
                        return None;
                    }
                    esub = esub.saturating_add(1);
                    ssub = esub.saturating_add(1);
                    esub = esub.saturating_add(opnd(m.g.word(esub as usize)) as Sopno);
                    if op(m.g.word(esub as usize)) == OOR2 {
                        esub = esub.saturating_sub(1);
                    }
                }
                let _ = dissect::<MULTIBYTE, S>(m, sp, rest, ssub, esub);
            }
            sp = rest;
        }
        ss = es;
    }
    if sp == stop { Some(sp) } else { None }
}

/// One logical `backref` call.
#[derive(Clone, Copy)]
struct BackrefCall {
    /// Subject position where the call starts.
    start: usize,
    /// Subject position where the call must stop.
    stop: usize,
    /// First strip index.
    startst: Sopno,
    /// One past the last strip index.
    stopst: Sopno,
    /// `+` nesting level.
    lev: Sopno,
    /// Empty-loop recursion counter (`MAX_RECURSION`).
    rec: i32,
}

/// What the parent does with a child call's result.
enum Resume {
    /// The child's result is the parent's result.
    Tail,
    /// Try the alternative call when the child fails.
    Quest {
        /// The branch to try next.
        alternative: BackrefCall,
    },
    /// Try the loop exit when the body fails.
    Plus {
        /// The loop-exit call.
        alternative: BackrefCall,
    },
    /// Try the next branch of a choice when this one fails.
    Choice {
        /// The current branch call; `stopst` is its `esub`.
        call: BackrefCall,
    },
    /// Restore one sub-match field when the child fails.
    Span {
        /// Sub-match slot.
        index: usize,
        /// Value to restore.
        saved: RegMatch,
        /// Restore `rm_so` (true) or `rm_eo` (false).
        so: bool,
    },
}

/// One logical recursion frame of [`backref`].
struct BackrefFrame {
    /// The continuation of the parent.
    resume: Resume,
}

/// What one logical call does after its linear part.
enum BackrefStep {
    /// The call is complete.
    Done(Option<usize>),
    /// The call needs a child call.
    Call {
        /// The child call.
        target: BackrefCall,
        /// The parent's continuation.
        resume: Resume,
    },
}

/// `backref`: figure out what matched what, figuring in back-references.
///
/// The donor recurses on every branch, loop iteration and parenthesis. Rust does not have
/// tail calls and the per-call frame is far too large for `BACKREF_DEPTH_MAX` nested calls
/// on a small thread stack, so the logical recursion is an explicit heap stack with the same
/// call order, the same budget charge per call and the same depth counter (H-RE-2). A deep
/// pattern returns `REG_ESPACE` without growing the machine stack.
fn backref<const MULTIBYTE: bool, S: StateSet>(
    m: &mut MatchState<'_, S>,
    start: usize,
    stop: usize,
    startst: Sopno,
    stopst: Sopno,
    lev: Sopno,
    rec: i32,
) -> Option<usize> {
    let mut stack: Vec<BackrefFrame> = Vec::new();
    let mut call = BackrefCall {
        start,
        stop,
        startst,
        stopst,
        lev,
        rec,
    };
    loop {
        if m.error != 0 {
            m.depth = 0;
            return None;
        }
        if m.budget == 0 {
            m.error = REG_ESPACE;
            m.depth = 0;
            return None;
        }
        m.budget = m.budget.saturating_sub(1);
        if m.budget == 0 {
            m.error = REG_ESPACE;
            m.depth = 0;
            return None;
        }
        if stack.len() as u32 >= BACKREF_DEPTH_MAX {
            m.error = REG_ESPACE;
            m.depth = 0;
            return None;
        }
        m.depth = (stack.len() as u32).saturating_add(1);
        match backref_step::<MULTIBYTE, S>(m, call) {
            BackrefStep::Done(value) => {
                let mut value = value;
                loop {
                    if m.error != 0 {
                        m.depth = 0;
                        return None;
                    }
                    let Some(frame) = stack.pop() else {
                        m.depth = 0;
                        return value;
                    };
                    m.depth = stack.len() as u32;
                    match frame.resume {
                        Resume::Tail => {}
                        Resume::Quest { alternative } => {
                            if value.is_some() {
                                continue;
                            }
                            call = alternative;
                            break;
                        }
                        Resume::Plus { alternative } => {
                            if value.is_some() {
                                continue;
                            }
                            call = alternative;
                            break;
                        }
                        Resume::Choice { call: current } => {
                            if value.is_some() {
                                continue;
                            }
                            let mut esub = current.stopst;
                            if op(m.g.word(esub as usize)) == O_CH {
                                value = None;
                                continue;
                            }
                            esub = esub.saturating_add(1);
                            let ssub = esub.saturating_add(1);
                            esub = esub.saturating_add(opnd(m.g.word(esub as usize)) as Sopno);
                            if op(m.g.word(esub as usize)) == OOR2 {
                                esub = esub.saturating_sub(1);
                            }
                            let next = BackrefCall {
                                start: current.start,
                                stop: current.stop,
                                startst: ssub,
                                stopst: esub,
                                lev: current.lev,
                                rec: current.rec,
                            };
                            stack.push(BackrefFrame {
                                resume: Resume::Choice { call: next },
                            });
                            call = next;
                            break;
                        }
                        Resume::Span { index, saved, so } => {
                            if value.is_none()
                                && let Some(table) = m.pmatch.as_mut()
                                && let Some(slot) = table.get_mut(index)
                            {
                                if so {
                                    slot.rm_so = saved.rm_so;
                                } else {
                                    slot.rm_eo = saved.rm_eo;
                                }
                            }
                            continue;
                        }
                    }
                }
            }
            BackrefStep::Call { target, resume } => {
                stack.push(BackrefFrame { resume });
                call = target;
            }
        }
    }
}

/// The linear part of one logical [`backref`] call.
#[allow(clippy::too_many_lines)]
fn backref_step<const MULTIBYTE: bool, S: StateSet>(
    m: &mut MatchState<'_, S>,
    call: BackrefCall,
) -> BackrefStep {
    let BackrefCall {
        start,
        stop,
        startst,
        stopst,
        lev,
        mut rec,
    } = call;
    let mut sp = start;
    let mut ss = startst;
    let mut hard = false;
    while !hard && ss < stopst {
        let word = m.g.word(ss as usize);
        let operator = op(word);
        if operator == OCHAR {
            if sp == stop {
                return BackrefStep::Done(None);
            }
            let (wc, used) = xmbrtowc::<MULTIBYTE>(
                m.loc,
                &mut m.mbs,
                m.subject.get(sp..stop).unwrap_or(&[]),
                BADCHAR,
            );
            if wc != opnd(word) as i32 {
                return BackrefStep::Done(None);
            }
            sp = sp.saturating_add(used);
        } else if operator == OANY {
            if sp == stop {
                return BackrefStep::Done(None);
            }
            let (wc, used) = xmbrtowc::<MULTIBYTE>(
                m.loc,
                &mut m.mbs,
                m.subject.get(sp..stop).unwrap_or(&[]),
                BADCHAR,
            );
            if wc == BADCHAR {
                return BackrefStep::Done(None);
            }
            sp = sp.saturating_add(used);
        } else if operator == OANYOF {
            if sp == stop {
                return BackrefStep::Done(None);
            }
            let index = opnd(word);
            let (wc, used) = xmbrtowc::<MULTIBYTE>(
                m.loc,
                &mut m.mbs,
                m.subject.get(sp..stop).unwrap_or(&[]),
                BADCHAR,
            );
            if wc == BADCHAR {
                return BackrefStep::Done(None);
            }
            let inside =
                m.g.sets
                    .get(index)
                    .is_some_and(|set| set.chin(wc, m.loc, m.nc));
            if !inside {
                return BackrefStep::Done(None);
            }
            sp = sp.saturating_add(used);
        } else if operator == OBOS {
            if !(sp == m.beginp && m.eflags & crate::consts::REG_NOTBOL == 0) {
                return BackrefStep::Done(None);
            }
        } else if operator == OEOS {
            if !(sp == m.endp && m.eflags & crate::consts::REG_NOTEOL == 0) {
                return BackrefStep::Done(None);
            }
        } else if operator == OBOL {
            let at_bol = (sp == m.beginp && m.eflags & crate::consts::REG_NOTBOL == 0)
                || (sp > 0
                    && sp < m.endp
                    && m.subject.get(sp.saturating_sub(1)).copied() == Some(b'\n')
                    && m.g.cflags & crate::consts::REG_NEWLINE != 0);
            if !at_bol {
                return BackrefStep::Done(None);
            }
        } else if operator == OEOL {
            let at_eol = (sp == m.endp && m.eflags & crate::consts::REG_NOTEOL == 0)
                || (sp < m.endp
                    && m.subject.get(sp).copied() == Some(b'\n')
                    && m.g.cflags & crate::consts::REG_NEWLINE != 0);
            if !at_eol {
                return BackrefStep::Done(None);
            }
        } else if operator == OWBND {
            if !(is_bow(m, sp) || is_eow(m, sp)) {
                return BackrefStep::Done(None);
            }
        } else if operator == crate::regex2::ONWBND {
            let previous = if sp > m.beginp {
                isword(
                    i32::from(m.subject.get(sp.saturating_sub(1)).copied().unwrap_or(0)),
                    m.loc,
                )
            } else {
                false
            };
            let current = if sp < m.endp {
                isword(i32::from(m.subject.get(sp).copied().unwrap_or(0)), m.loc)
            } else {
                false
            };
            let ok =
                (sp == m.beginp && !current) || (sp == m.endp && !previous) || previous == current;
            if !ok {
                return BackrefStep::Done(None);
            }
        } else if operator == crate::regex2::OBOW {
            if !is_bow(m, sp) {
                return BackrefStep::Done(None);
            }
        } else if operator == crate::regex2::OEOW {
            if !is_eow(m, sp) {
                return BackrefStep::Done(None);
            }
        } else if operator == O_QUEST {
        } else if operator == OOR1 {
            ss = ss.saturating_add(1);
            let mut inner = m.g.word(ss as usize);
            loop {
                if op(inner) != OOR2 {
                    return BackrefStep::Done(None);
                }
                ss = ss.saturating_add(opnd(inner) as Sopno);
                inner = m.g.word(ss as usize);
                if op(inner) == O_CH {
                    break;
                }
            }
        } else {
            hard = true;
            ss = ss.saturating_add(1);
            break;
        }
        ss = ss.saturating_add(1);
    }
    if !hard {
        return BackrefStep::Done(if sp == stop { Some(sp) } else { None });
    }
    ss = ss.saturating_sub(1);

    let word = m.g.word(ss as usize);
    let operator = op(word);
    if operator == OBACK_ {
        let index = opnd(word);
        let entry = m
            .pmatch
            .as_ref()
            .and_then(|table| table.get(index))
            .copied()
            .unwrap_or(RegMatch {
                rm_so: -1,
                rm_eo: -1,
            });
        if entry.rm_eo == -1 {
            return BackrefStep::Done(None);
        }
        if entry.rm_so == -1 {
            return BackrefStep::Done(None);
        }
        let len = match usize::try_from(entry.rm_eo.saturating_sub(entry.rm_so)) {
            Ok(value) => value,
            Err(_) => return BackrefStep::Done(None),
        };
        if len == 0 {
            rec = rec.saturating_add(1);
            if rec > MAX_RECURSION {
                return BackrefStep::Done(None);
            }
        }
        if sp > stop.saturating_sub(len) {
            return BackrefStep::Done(None);
        }
        let ssp = match usize::try_from(entry.rm_so) {
            Ok(value) => value,
            Err(_) => return BackrefStep::Done(None),
        };
        let left = m.subject.get(sp..sp.saturating_add(len));
        let right = m.subject.get(ssp..ssp.saturating_add(len));
        if left != right {
            return BackrefStep::Done(None);
        }
        while m.g.word(ss as usize) != sop(O_BACK, index) {
            if ss as usize >= m.g.strip.len() {
                return BackrefStep::Done(None);
            }
            ss = ss.saturating_add(1);
        }
        BackrefStep::Call {
            target: BackrefCall {
                start: sp.saturating_add(len),
                stop,
                startst: ss.saturating_add(1),
                stopst,
                lev,
                rec,
            },
            resume: Resume::Tail,
        }
    } else if operator == OQUEST_ {
        BackrefStep::Call {
            target: BackrefCall {
                start: sp,
                stop,
                startst: ss.saturating_add(1),
                stopst,
                lev,
                rec,
            },
            resume: Resume::Quest {
                alternative: BackrefCall {
                    start: sp,
                    stop,
                    startst: ss.saturating_add(opnd(word) as Sopno).saturating_add(1),
                    stopst,
                    lev,
                    rec,
                },
            },
        }
    } else if operator == OPLUS_ {
        if let Some(slot) = m.lastpos.get_mut((lev as usize).saturating_add(1)) {
            *slot = sp;
        }
        BackrefStep::Call {
            target: BackrefCall {
                start: sp,
                stop,
                startst: ss.saturating_add(1),
                stopst,
                lev: lev.saturating_add(1),
                rec,
            },
            resume: Resume::Tail,
        }
    } else if operator == O_PLUS {
        let last = m.lastpos.get(lev as usize).copied().unwrap_or(usize::MAX);
        let exit = BackrefCall {
            start: sp,
            stop,
            startst: ss.saturating_add(1),
            stopst,
            lev: lev.saturating_sub(1),
            rec,
        };
        if sp == last {
            return BackrefStep::Call {
                target: exit,
                resume: Resume::Tail,
            };
        }
        if let Some(slot) = m.lastpos.get_mut(lev as usize) {
            *slot = sp;
        }
        BackrefStep::Call {
            target: BackrefCall {
                start: sp,
                stop,
                startst: ss.saturating_sub(opnd(word) as Sopno).saturating_add(1),
                stopst,
                lev,
                rec,
            },
            resume: Resume::Plus { alternative: exit },
        }
    } else if operator == OCH_ {
        let ssub = ss.saturating_add(1);
        let esub = ss.saturating_add(opnd(word) as Sopno).saturating_sub(1);
        let branch = BackrefCall {
            start: sp,
            stop,
            startst: ssub,
            stopst: esub,
            lev,
            rec,
        };
        BackrefStep::Call {
            target: branch,
            resume: Resume::Choice { call: branch },
        }
    } else if operator == OLPAREN || operator == ORPAREN {
        let index = opnd(word);
        let saved = m
            .pmatch
            .as_ref()
            .and_then(|table| table.get(index))
            .copied()
            .unwrap_or(RegMatch {
                rm_so: -1,
                rm_eo: -1,
            });
        if let Some(table) = m.pmatch.as_mut()
            && let Some(slot) = table.get_mut(index)
        {
            if operator == OLPAREN {
                slot.rm_so = sp as i64;
            } else {
                slot.rm_eo = sp as i64;
            }
        }
        BackrefStep::Call {
            target: BackrefCall {
                start: sp,
                stop,
                startst: ss.saturating_add(1),
                stopst,
                lev,
                rec,
            },
            resume: Resume::Span {
                index,
                saved,
                so: operator == OLPAREN,
            },
        }
    } else {
        BackrefStep::Done(None)
    }
}

/// `walk`: step through the subject, quickly or slowly.
#[allow(clippy::too_many_lines)]
fn walk<const MULTIBYTE: bool, S: StateSet>(
    m: &mut MatchState<'_, S>,
    start: usize,
    stop: usize,
    startst: Sopno,
    stopst: Sopno,
    fast: bool,
) -> Option<usize> {
    let mut st: S = m.st.clone();
    let mut fresh: S = m.fresh.clone();
    let empty: S = m.empty.clone();
    let mut tmp: S = m.tmp.clone();
    st.clear();
    st.set1(startst as usize);
    step_self::<S>(m.g, startst, stopst, &mut st, NOTHING, 0, m.loc, m.nc);
    if fast {
        fresh.assign(&st);
    }
    let mut matchp: Option<usize> = None;
    let mut p = start;
    let mut c = if start == 0 || (start == m.beginp && m.eflags & crate::consts::REG_NOTBOL == 0) {
        OUT
    } else {
        i32::from(m.subject.get(start.saturating_sub(1)).copied().unwrap_or(0))
    };
    loop {
        let lastc = c;
        let mut sflags = 0i32;
        let mut clen = 0usize;
        if p == m.endp {
            c = OUT;
        } else {
            let (value, used) = xmbrtowc::<MULTIBYTE>(
                m.loc,
                &mut m.mbs,
                m.subject.get(p..m.endp).unwrap_or(&[]),
                BADCHAR,
            );
            c = value;
            clen = used;
        }

        if fast && st.eq(&fresh) {
            matchp = Some(p);
        }

        let mut flagch = 0i32;
        let mut count = 0i32;
        if (lastc == i32::from(b'\n') && m.g.cflags & crate::consts::REG_NEWLINE != 0)
            || (lastc == OUT && m.eflags & crate::consts::REG_NOTBOL == 0)
        {
            flagch = BOL;
            count = i32::try_from(m.g.nbol).unwrap_or(i32::MAX);
        }
        if (c == i32::from(b'\n') && m.g.cflags & crate::consts::REG_NEWLINE != 0)
            || (c == OUT && m.eflags & crate::consts::REG_NOTEOL == 0)
        {
            flagch = if flagch == BOL { BOLEOL } else { EOL };
            count = count.saturating_add(i32::try_from(m.g.neol).unwrap_or(i32::MAX));
        }
        if lastc == OUT && m.eflags & crate::consts::REG_NOTBOL == 0 {
            sflags |= SBOS;
            count = count.saturating_add(1);
        }
        if c == OUT && m.eflags & crate::consts::REG_NOTEOL == 0 {
            sflags |= SEOS;
            count = count.saturating_add(1);
        }
        for _ in 0..count {
            step_self::<S>(m.g, startst, stopst, &mut st, flagch, sflags, m.loc, m.nc);
        }

        if (flagch == BOL || (lastc != OUT && !isword(lastc, m.loc)))
            && (c != OUT && isword(c, m.loc))
        {
            flagch = BOW;
        }
        if (lastc != OUT && isword(lastc, m.loc))
            && (flagch == EOL || (c != OUT && !isword(c, m.loc)))
        {
            flagch = EOW;
        }
        if flagch == BOW || flagch == EOW {
            step_self::<S>(m.g, startst, stopst, &mut st, flagch, sflags, m.loc, m.nc);
        }

        let not_a_boundary = (lastc != OUT && c != OUT && isword(lastc, m.loc) == isword(c, m.loc))
            || (lastc == OUT && !isword(c, m.loc))
            || (c == OUT && !isword(lastc, m.loc));
        if not_a_boundary {
            flagch = NWBND;
        }
        if flagch == NWBND {
            step_self::<S>(m.g, startst, stopst, &mut st, flagch, sflags, m.loc, m.nc);
        }

        if st.is_set(stopst as usize) {
            if fast {
                break;
            }
            matchp = Some(p);
        }
        if st.eq(&empty) || p == stop || clen > stop.saturating_sub(p) {
            break;
        }

        tmp.assign(&st);
        if fast {
            st.assign(&fresh);
        } else {
            st.clear();
        }
        step_from::<S>(m.g, startst, stopst, &tmp, &mut st, c, sflags, m.loc, m.nc);
        p = p.saturating_add(clen);
    }

    if fast {
        m.coldp = matchp?;
        if st.is_set(stopst as usize) {
            let (_, used) =
                xmbrtowc::<MULTIBYTE>(m.loc, &mut m.mbs, m.subject.get(p..).unwrap_or(&[]), 0);
            Some(p.saturating_add(used))
        } else {
            None
        }
    } else {
        matchp
    }
}

/// `step` with the set as its own source (`states` passed twice in the donor).
#[allow(clippy::too_many_arguments)] // mirrors the donor's state-macro expansion
fn step_self<S: StateSet>(
    g: &ReGuts,
    start: Sopno,
    stop: Sopno,
    set: &mut S,
    ch: i32,
    sflags: i32,
    loc: &dyn RegexLocale,
    nc: usize,
) {
    let mut pc = start;
    while pc != stop {
        let word = g.word(pc as usize);
        let operator = op(word);
        if operator == OCHAR {
            if ch == opnd(word) as i32 {
                set.fwd_self(pc as usize, 1);
            }
        } else if operator == OBOS {
            if (ch == BOL || ch == BOLEOL) && sflags & SBOS != 0 {
                set.fwd_self(pc as usize, 1);
            }
        } else if operator == OEOS {
            if (ch == EOL || ch == BOLEOL) && sflags & SEOS != 0 {
                set.fwd_self(pc as usize, 1);
            }
        } else if operator == OBOL {
            if ch == BOL || ch == BOLEOL {
                set.fwd_self(pc as usize, 1);
            }
        } else if operator == OEOL {
            if ch == EOL || ch == BOLEOL {
                set.fwd_self(pc as usize, 1);
            }
        } else if operator == OWBND {
            if ch == BOW {
                set.fwd_self(pc as usize, 1);
            }
        } else if operator == crate::regex2::OEOW {
            if ch == EOW {
                set.fwd_self(pc as usize, 1);
            }
        } else if operator == crate::regex2::ONWBND {
            if ch == NWBND {
                set.fwd_self(pc as usize, 1);
            }
        } else if operator == OANY {
            if !nonchar(ch) {
                set.fwd_self(pc as usize, 1);
            }
        } else if operator == OANYOF {
            if !nonchar(ch)
                && g.sets
                    .get(opnd(word))
                    .is_some_and(|cs| cs.chin(ch, loc, nc))
            {
                set.fwd_self(pc as usize, 1);
            }
        } else if operator == OBACK_ || operator == O_BACK || operator == OPLUS_ {
            set.fwd_self(pc as usize, 1);
        } else if operator == O_PLUS {
            set.fwd_self(pc as usize, 1);
            let distance = opnd(word);
            let was = set.issetback(pc as usize, distance);
            set.back_self(pc as usize, distance);
            if !was && set.issetback(pc as usize, distance) {
                pc = pc.saturating_sub(distance as Sopno).saturating_sub(1);
                continue;
            }
        } else if operator == OQUEST_ {
            set.fwd_self(pc as usize, 1);
            set.fwd_self(pc as usize, opnd(word));
        } else if operator == O_QUEST
            || operator == OLPAREN
            || operator == ORPAREN
            || operator == O_CH
        {
            set.fwd_self(pc as usize, 1);
        } else if operator == OCH_ {
            set.fwd_self(pc as usize, 1);
            set.fwd_self(pc as usize, opnd(word));
        } else if operator == OOR1 {
            if set.is_state_in(pc as usize) {
                let mut look = 1usize;
                loop {
                    let inner = g.word((pc as usize).saturating_add(look));
                    if op(inner) == O_CH || op(inner) == OEND {
                        break;
                    }
                    look = look.saturating_add(opnd(inner));
                }
                set.fwd_self(pc as usize, look.saturating_add(1));
            }
        } else if operator == OOR2 {
            set.fwd_self(pc as usize, 1);
            if op(g.word((pc as usize).saturating_add(opnd(word)))) != O_CH {
                set.fwd_self(pc as usize, opnd(word));
            }
        }
        pc = pc.saturating_add(1);
    }
}

/// `step` with a distinct source set (`tmp` to `st` in `walk`).
#[allow(clippy::too_many_arguments)] // mirrors the donor's state-macro expansion
fn step_from<S: StateSet>(
    g: &ReGuts,
    start: Sopno,
    stop: Sopno,
    before: &S,
    after: &mut S,
    ch: i32,
    sflags: i32,
    loc: &dyn RegexLocale,
    nc: usize,
) {
    let mut pc = start;
    while pc != stop {
        let word = g.word(pc as usize);
        let operator = op(word);
        if operator == OCHAR {
            if ch == opnd(word) as i32 {
                after.fwd_from(before, pc as usize, 1);
            }
        } else if operator == OBOS {
            if (ch == BOL || ch == BOLEOL) && sflags & SBOS != 0 {
                after.fwd_from(before, pc as usize, 1);
            }
        } else if operator == OEOS {
            if (ch == EOL || ch == BOLEOL) && sflags & SEOS != 0 {
                after.fwd_from(before, pc as usize, 1);
            }
        } else if operator == OBOL {
            if ch == BOL || ch == BOLEOL {
                after.fwd_from(before, pc as usize, 1);
            }
        } else if operator == OEOL {
            if ch == EOL || ch == BOLEOL {
                after.fwd_from(before, pc as usize, 1);
            }
        } else if operator == OWBND {
            if ch == BOW {
                after.fwd_from(before, pc as usize, 1);
            }
        } else if operator == crate::regex2::OEOW {
            if ch == EOW {
                after.fwd_from(before, pc as usize, 1);
            }
        } else if operator == crate::regex2::ONWBND {
            if ch == NWBND {
                after.fwd_from(before, pc as usize, 1);
            }
        } else if operator == OANY {
            if !nonchar(ch) {
                after.fwd_from(before, pc as usize, 1);
            }
        } else if operator == OANYOF {
            if !nonchar(ch)
                && g.sets
                    .get(opnd(word))
                    .is_some_and(|cs| cs.chin(ch, loc, nc))
            {
                after.fwd_from(before, pc as usize, 1);
            }
        } else if operator == OBACK_ || operator == O_BACK || operator == OPLUS_ {
            after.fwd_self(pc as usize, 1);
        } else if operator == O_PLUS {
            after.fwd_self(pc as usize, 1);
            let distance = opnd(word);
            let was = after.issetback(pc as usize, distance);
            after.back_self(pc as usize, distance);
            if !was && after.issetback(pc as usize, distance) {
                pc = pc.saturating_sub(distance as Sopno).saturating_sub(1);
                continue;
            }
        } else if operator == OQUEST_ {
            after.fwd_self(pc as usize, 1);
            after.fwd_self(pc as usize, opnd(word));
        } else if operator == O_QUEST
            || operator == OLPAREN
            || operator == ORPAREN
            || operator == O_CH
        {
            after.fwd_self(pc as usize, 1);
        } else if operator == OCH_ {
            after.fwd_self(pc as usize, 1);
            after.fwd_self(pc as usize, opnd(word));
        } else if operator == OOR1 {
            if after.is_state_in(pc as usize) {
                let mut look = 1usize;
                loop {
                    let inner = g.word((pc as usize).saturating_add(look));
                    if op(inner) == O_CH || op(inner) == OEND {
                        break;
                    }
                    look = look.saturating_add(opnd(inner));
                }
                after.fwd_self(pc as usize, look.saturating_add(1));
            }
        } else if operator == OOR2 {
            after.fwd_self(pc as usize, 1);
            if op(g.word((pc as usize).saturating_add(opnd(word)))) != O_CH {
                after.fwd_self(pc as usize, opnd(word));
            }
        }
        pc = pc.saturating_add(1);
    }
}

#[cfg(test)]
#[path = "engine_ut.rs"]
mod tests;
