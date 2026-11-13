// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/regex/fnmatch.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file header upstream; covered by musl/COPYRIGHT, H-1)
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

//! `fnmatch` with the musl "Sea of Stars" algorithm and the FreeBSD flag values.
//!
//! The pattern is factored into a head, a tail and the star-separated components between them;
//! each component is searched forward once, so the worst case is linear-ish and never
//! exponential (A-glob-2, doc 07 4.3). Flag *values* are the FreeBSD header's (A-glob-3), the
//! locale is a [`PatternLocale`] and invalid bytes in the string are unmatchable
//! (`UNMATCHABLE`, `fnmatch.c:23`). The function returns only `0` or [`FNM_NOMATCH`].

use crate::consts::{
    FNM_CASEFOLD, FNM_LEADING_DIR, FNM_NOESCAPE, FNM_NOMATCH, FNM_PATHNAME, FNM_PERIOD,
};
use crate::locale::{MbResult, PatternLocale};

const END: i32 = 0;
const UNMATCHABLE: i32 = -2;
const BRACKET: i32 = -3;
const QUESTION: i32 = -4;
const STAR: i32 = -5;

/// One past `index`.
fn next(index: usize) -> usize {
    index.wrapping_add(1)
}

/// One before `index`; callers only use it after a check that keeps it in range.
fn prev(index: usize) -> usize {
    index.wrapping_sub(1)
}

/// The byte slice from `start`, or empty.
fn tail(s: &[u8], start: usize) -> &[u8] {
    s.get(start..).unwrap_or(&[])
}

/// The byte slice `start..end`, or empty.
fn span(s: &[u8], start: usize, end: usize) -> &[u8] {
    s.get(start..end).unwrap_or(&[])
}

/// The byte at `index`, or `0` past the end: the donor's NUL terminator.
fn at(s: &[u8], index: usize) -> u8 {
    s.get(index).copied().unwrap_or(0)
}

/// `strnlen`: the length of `s` up to its first NUL.
fn strnlen(s: &[u8]) -> usize {
    s.iter().position(|&b| b == 0).unwrap_or(s.len())
}

/// Decode one string character: `(value, step)`; `-1` means an undecodable byte.
fn str_next(loc: &dyn PatternLocale, s: &[u8]) -> (i32, usize) {
    let Some(&first) = s.first() else {
        return (0, 0);
    };
    if first >= 128 {
        return match loc.mbtowc(s) {
            MbResult::Char { wc, len } => (wc as i32, len),
            MbResult::Incomplete | MbResult::Invalid => (-1, 1),
        };
    }
    (i32::from(first), 1)
}

/// Parse one pattern element: `(kind-or-code-point, step)`.
fn pat_next(loc: &dyn PatternLocale, pat: &[u8], m: usize, flags: u32) -> (i32, usize) {
    let m = m.min(pat.len());
    if m == 0 || at(pat, 0) == 0 {
        return (END, 0);
    }
    let mut step = 1;
    let mut start = 0;
    let mut esc = false;
    if at(pat, 0) == b'\\' && at(pat, 1) != 0 && flags & FNM_NOESCAPE == 0 {
        step = 2;
        start = 1;
        esc = true;
    } else if at(pat, 0) == b'[' {
        let mut k = 1;
        if k < m && (at(pat, k) == b'^' || at(pat, k) == b'!') {
            k = next(k);
        }
        if k < m && at(pat, k) == b']' {
            k = next(k);
        }
        while k < m && at(pat, k) != 0 && at(pat, k) != b']' {
            if next(k) < m
                && at(pat, next(k)) != 0
                && at(pat, k) == b'['
                && (at(pat, next(k)) == b':'
                    || at(pat, next(k)) == b'.'
                    || at(pat, next(k)) == b'=')
            {
                let z = at(pat, next(k));
                k = k.wrapping_add(2);
                if k < m && at(pat, k) != 0 {
                    k = next(k);
                }
                while k < m && at(pat, k) != 0 && (at(pat, prev(k)) != z || at(pat, k) != b']') {
                    k = next(k);
                }
                if k == m || at(pat, k) == 0 {
                    break;
                }
            }
            k = next(k);
        }
        if k == m || at(pat, k) == 0 {
            return (i32::from(b'['), 1);
        }
        return (BRACKET, next(k));
    }
    if at(pat, 0) == b'*' {
        return (STAR, 1);
    }
    if at(pat, 0) == b'?' {
        return (QUESTION, 1);
    }
    let Some(&first) = pat.get(start) else {
        return (END, 0);
    };
    if first >= 128 {
        let rest = pat.get(start..m).unwrap_or(&[]);
        return match loc.mbtowc(rest) {
            MbResult::Char { wc, len } => (wc as i32, len.wrapping_add(usize::from(esc))),
            MbResult::Incomplete | MbResult::Invalid => (UNMATCHABLE, 0),
        };
    }
    (i32::from(first), step)
}

/// `casefold()`: uppercase unless it is a no-op, then lowercase (`fnmatch.c:101-104`).
fn casefold(loc: &dyn PatternLocale, k: i32) -> i32 {
    let k = k as u32;
    let c = loc.towupper(k);
    if c == k {
        loc.towlower(k) as i32
    } else {
        c as i32
    }
}

/// Match one bracket expression. `p` is the exact token, starting at `[`.
fn match_bracket(loc: &dyn PatternLocale, p: &[u8], k: i32, kfold: i32) -> bool {
    let mut index = 1usize;
    let mut inv = false;
    if at(p, index) == b'^' || at(p, index) == b'!' {
        inv = true;
        index = next(index);
    }
    if at(p, index) == b']' {
        if k == i32::from(b']') {
            return !inv;
        }
        index = next(index);
    } else if at(p, index) == b'-' {
        if k == i32::from(b'-') {
            return !inv;
        }
        index = next(index);
    }
    let mut wc = i32::from(at(p, prev(index)));
    while index < p.len() && at(p, index) != b']' {
        if at(p, index) == b'-' && at(p, next(index)) != b']' {
            let Some(rest) = p.get(next(index)..) else {
                return false;
            };
            let wc2 = match loc.mbtowc(rest) {
                MbResult::Char { wc, len } => {
                    index = index.wrapping_add(len).wrapping_add(1);
                    wc as i32
                }
                MbResult::Incomplete | MbResult::Invalid => return false,
            };
            if wc <= wc2 {
                let span = wc2.wrapping_sub(wc) as u32;
                if (k as u32).wrapping_sub(wc as u32) <= span
                    || (kfold as u32).wrapping_sub(wc as u32) <= span
                {
                    return !inv;
                }
            }
            continue;
        }
        if at(p, index) == b'['
            && (at(p, next(index)) == b':'
                || at(p, next(index)) == b'.'
                || at(p, next(index)) == b'=')
        {
            let p0 = index.wrapping_add(2);
            let z = at(p, next(index));
            index = index.wrapping_add(3);
            while index < p.len() && (at(p, prev(index)) != z || at(p, index) != b']') {
                index = next(index);
            }
            if z == b':' && prev(index).wrapping_sub(p0) < 16 && index > p0 {
                let name = p.get(p0..prev(index)).unwrap_or(&[]);
                if let Some(t) = loc.wctype(name)
                    && (loc.iswctype(k as u32, t) || loc.iswctype(kfold as u32, t))
                {
                    return !inv;
                }
            }
            index = next(index);
            continue;
        }
        if at(p, index) < 128 {
            wc = i32::from(at(p, index));
            index = next(index);
        } else {
            let Some(rest) = p.get(index..) else {
                return false;
            };
            match loc.mbtowc(rest) {
                MbResult::Char { wc: c, len } => {
                    wc = c as i32;
                    index = index.wrapping_add(len);
                }
                MbResult::Incomplete | MbResult::Invalid => return false,
            }
        }
        if (wc == k || wc == kfold) && (at(p, index) != b'-' || at(p, next(index)) == b']') {
            return !inv;
        }
    }
    inv
}

/// The body of `fnmatch` over exact slices (`fnmatch_internal`).
fn fnmatch_internal(loc: &dyn PatternLocale, pat: &[u8], s: &[u8], flags: u32) -> i32 {
    let patlen = strnlen(pat);
    let pat = span(pat, 0, patlen);
    let strlen = strnlen(s);
    let s = span(s, 0, strlen);

    if flags & FNM_PERIOD != 0
        && s.first() == Some(&b'.')
        && pat.first() != Some(&b'.')
        && pat.first() != Some(&b'\\')
    {
        return FNM_NOMATCH;
    }

    let mut pi = 0usize;
    let mut si = 0usize;
    loop {
        let c = pat_next(loc, tail(pat, pi), patlen.wrapping_sub(pi), flags);
        match c.0 {
            UNMATCHABLE => return FNM_NOMATCH,
            STAR => pi = next(pi),
            _ => {
                let (k, sinc) = str_next(loc, tail(s, si));
                if k <= 0 {
                    return if c.0 == END && k == 0 { 0 } else { FNM_NOMATCH };
                }
                si = si.wrapping_add(sinc);
                let kfold = if flags & FNM_CASEFOLD != 0 {
                    casefold(loc, k)
                } else {
                    k
                };
                if c.0 == BRACKET {
                    let token = pat.get(pi..pi.wrapping_add(c.1)).unwrap_or(&[]);
                    if !match_bracket(loc, token, k, kfold) {
                        return FNM_NOMATCH;
                    }
                } else if c.0 != QUESTION && k != c.0 && kfold != c.0 {
                    return FNM_NOMATCH;
                }
                pi = pi.wrapping_add(c.1);
                continue;
            }
        }
        break;
    }

    let endpat = patlen;

    let mut p = pi;
    let mut ptail = pi;
    let mut tailcnt = 0usize;
    while p < endpat {
        let (c, pinc) = pat_next(loc, tail(pat, p), endpat.wrapping_sub(p), flags);
        match c {
            UNMATCHABLE => return FNM_NOMATCH,
            STAR => {
                tailcnt = 0;
                ptail = next(p);
            }
            _ => tailcnt = tailcnt.saturating_add(1),
        }
        if pinc == 0 {
            break;
        }
        p = p.wrapping_add(pinc);
    }

    let endstr = s.len();
    if endstr.saturating_sub(si) < tailcnt {
        return FNM_NOMATCH;
    }
    let mut stail = endstr;
    let mut left = tailcnt;
    while stail > si && left > 0 {
        if at(s, prev(stail)) < 128 || loc.mb_cur_max() == 1 {
            stail = prev(stail);
        } else {
            loop {
                stail = prev(stail);
                if !(at(s, stail).wrapping_sub(0x80) < 0x40 && stail > si) {
                    break;
                }
            }
        }
        left = prev(left);
    }
    if left > 0 {
        return FNM_NOMATCH;
    }

    p = ptail;
    let mut tail_s = stail;
    loop {
        let (c, pinc) = pat_next(loc, tail(pat, p), endpat.wrapping_sub(p), flags);
        let token_start = p;
        p = p.wrapping_add(pinc);
        let (k, sinc) = str_next(loc, tail(s, tail_s));
        if k <= 0 {
            if c != END || k < 0 {
                return FNM_NOMATCH;
            }
            break;
        }
        tail_s = tail_s.wrapping_add(sinc);
        let kfold = if flags & FNM_CASEFOLD != 0 {
            casefold(loc, k)
        } else {
            k
        };
        if c == BRACKET {
            let token = pat
                .get(token_start..token_start.wrapping_add(pinc))
                .unwrap_or(&[]);
            if !match_bracket(loc, token, k, kfold) {
                return FNM_NOMATCH;
            }
        } else if c != QUESTION && k != c && kfold != c {
            return FNM_NOMATCH;
        }
    }

    let endstr = stail;
    let endpat = ptail;

    while pi < endpat {
        p = pi;
        let mut s_i = si;
        let mut star = false;
        loop {
            let (c, pinc) = pat_next(loc, tail(pat, p), endpat.wrapping_sub(p), flags);
            let token_start = p;
            p = p.wrapping_add(pinc);
            if c == STAR {
                pi = p;
                si = s_i;
                star = true;
                break;
            }
            let (k, sinc) = str_next(loc, span(s, s_i, endstr));
            if k == 0 {
                return FNM_NOMATCH;
            }
            let kfold = if flags & FNM_CASEFOLD != 0 {
                casefold(loc, k)
            } else {
                k
            };
            if c == BRACKET {
                let token = pat
                    .get(token_start..token_start.wrapping_add(pinc))
                    .unwrap_or(&[]);
                if !match_bracket(loc, token, k, kfold) {
                    break;
                }
            } else if c != QUESTION && k != c && kfold != c {
                break;
            }
            s_i = s_i.wrapping_add(sinc);
        }
        if star {
            continue;
        }
        let (k, sinc) = str_next(loc, span(s, si, endstr));
        if k > 0 {
            si = si.wrapping_add(sinc);
        } else {
            si = next(si);
            while si < endstr && str_next(loc, span(s, si, endstr)).0 < 0 {
                si = next(si);
            }
        }
    }

    0
}

/// POSIX `fnmatch` with the FreeBSD flag values; returns `0` or [`FNM_NOMATCH`].
#[must_use]
pub fn fnmatch(pat: &[u8], s: &[u8], flags: u32, loc: &dyn PatternLocale) -> i32 {
    let mut si = 0usize;
    let mut pi = 0usize;
    if flags & FNM_PATHNAME != 0 {
        loop {
            let mut s_end = si;
            while s_end < s.len() && at(s, s_end) != 0 && at(s, s_end) != b'/' {
                s_end = next(s_end);
            }
            let mut p = pi;
            let (c, inc) = loop {
                let (c, inc) = pat_next(loc, tail(pat, p), pat.len().wrapping_sub(p), flags);
                if c == END || c == i32::from(b'/') {
                    break (c, inc);
                }
                p = p.wrapping_add(inc);
            };
            let sc = at(s, s_end);
            if c != i32::from(sc) && (sc == 0 || flags & FNM_LEADING_DIR == 0) {
                return FNM_NOMATCH;
            }
            if fnmatch_internal(loc, span(pat, pi, p), span(s, si, s_end), flags) != 0 {
                return FNM_NOMATCH;
            }
            if c == END {
                return 0;
            }
            si = next(s_end);
            pi = p.wrapping_add(inc);
        }
    } else if flags & FNM_LEADING_DIR != 0 {
        for (offset, &byte) in tail(s, si).iter().enumerate() {
            if byte != b'/' {
                continue;
            }
            if fnmatch_internal(
                loc,
                tail(pat, pi),
                span(s, si, si.wrapping_add(offset)),
                flags,
            ) == 0
            {
                return 0;
            }
        }
    }
    fnmatch_internal(loc, tail(pat, pi), tail(s, si), flags)
}

#[cfg(test)]
#[path = "fnmatch_ut.rs"]
mod tests;
