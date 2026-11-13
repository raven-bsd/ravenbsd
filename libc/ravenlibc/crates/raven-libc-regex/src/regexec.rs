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
//	@(#)regexec.c	8.3 (Berkeley) 3/20/94
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/regex/regexec.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// Altered: transpiled to Rust; hardening limits MAX_STRIP, BACKREF_BUDGET, MAX_PAREN_DEPTH, BACKREF_DEPTH_MAX; generic state-set engine; wide-character access through RegexLocale.

//! The `regexec` dispatcher (`regexec.c`).
//!
//! The donor includes `engine.c` three times after redefining the state macros; the Rust
//! engine is generic and this module only picks the representation, exactly as the donor's
//! `regexec` does (`regexec.c:248-253`): multibyte locale -> `mmatcher`; otherwise
//! `nstates <= 64 && !REG_LARGE` -> `smatcher`; else `lmatcher`. `xmbrtowc` lives in
//! [`crate::engine`] because all three families share it.
//!
//! `REG_LARGE` and `REG_BACKR` are kept live: the donor's non-debug `GOODFLAGS` masks them
//! away (they are dead there), but the BSD interface and ticket 5.2/5.3 require them, so the
//! mask here keeps the three execution flags plus the two matcher selectors.

use crate::consts::{
    BACKREF_BUDGET, REG_BACKR, REG_BADPAT, REG_INVARG, REG_LARGE, REG_NOTBOL, REG_NOTEOL,
    REG_STARTEND,
};
use crate::engine::{LargeSet, SmallSet, matcher};
use crate::locale::RegexLocale;
use crate::{RegError, RegMatch, Regex};

/// The execution flags the engine observes. `REG_TRACE` is accepted but ignored.
const EFLAG_MASK: u32 = REG_NOTBOL | REG_NOTEOL | REG_STARTEND | REG_LARGE | REG_BACKR;

/// `regexec`: match `string` against `re`.
///
/// `pmatch.len()` is the donor's `nmatch`; an empty slice means no sub-match output and no
/// writes to it (`REG_NOSUB` behaviour). With `REG_STARTEND`, `pmatch[0]` is input and output:
/// the search runs over `[rm_so, rm_eo)` and the returned offsets are relative to
/// `string[0]` (A-rx-7). `Err(RegError(REG_NOMATCH))` is the no-match result.
///
/// # Errors
///
/// `REG_NOMATCH` when there is no match; `REG_INVARG` for an invalid magic word, an invalid
/// `REG_STARTEND` window or a missing `pmatch[0]`; `REG_BADPAT` for an inconsistent strip;
/// `REG_ESPACE` when the back-reference budget or a limit is exhausted.
pub fn regexec(
    re: &Regex,
    string: &[u8],
    pmatch: &mut [RegMatch],
    eflags: u32,
    loc: &dyn RegexLocale,
) -> Result<(), RegError> {
    regexec_limited(re, string, pmatch, eflags, loc, BACKREF_BUDGET)
}

/// The internal entry point with an explicit budget, for the exact-budget unit test.
pub(crate) fn regexec_limited(
    re: &Regex,
    string: &[u8],
    pmatch: &mut [RegMatch],
    eflags: u32,
    loc: &dyn RegexLocale,
    budget: u64,
) -> Result<(), RegError> {
    if !re.magic_ok() {
        return Err(RegError(REG_INVARG));
    }
    let g = &re.guts;
    if g.iflags & crate::consts::BAD != 0 {
        return Err(RegError(REG_BADPAT));
    }
    let eflags = eflags & EFLAG_MASK;

    if loc.mb_cur_max() > 1 {
        matcher::<true, LargeSet>(g, string, pmatch, eflags, loc, budget)
    } else if g.nstates <= 64 && eflags & REG_LARGE == 0 {
        matcher::<false, SmallSet>(g, string, pmatch, eflags, loc, budget)
    } else {
        matcher::<false, LargeSet>(g, string, pmatch, eflags, loc, budget)
    }
}

#[cfg(test)]
#[path = "regexec_ut.rs"]
mod tests;
