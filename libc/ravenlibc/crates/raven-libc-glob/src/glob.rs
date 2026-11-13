// SPDX-License-Identifier: BSD-3-Clause
//
// Copyright (c) 1989, 1993
//	The Regents of the University of California.  All rights reserved.
//
// This code is derived from software contributed to Berkeley by
// Guido van Rossum.
//
// Copyright (c) 2011 The FreeBSD Foundation
//
// Portions of this software were developed by David Chisnall
// under sponsorship from the FreeBSD Foundation.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/gen/glob.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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

//! `glob` and `globfree` with the FreeBSD expansion engine.
//!
//! Pattern parsing is the donor's `__glob` (protect quoted characters, decode to `Char`),
//! brace expansion is `globexp0/1/2`, tilde expansion is `globtilde`, the compiled matcher is
//! `match` (`A-glob-2`: this is *not* `fnmatch`), and `glob2`/`glob3` walk the path segments.
//!
//! Adaptations from the donor: `strcoll` sorting becomes byte order; `mbstate_t`/`mbrtowc`
//! become [`PatternLocale`]; `secure_getenv`/`getlogin`/`getpwnam`/`getpwuid` become
//! [`GlobEnv`]; `g_opendir`/`g_lstat`/`g_stat` become [`DirSys`]; `errno` is carried in
//! [`GlobState::errno`] for the shell. `GLOB_LIMIT` arithmetic is the donor's, plus H-GL-1
//! (the brace limit applies even without the flag). `GLOB_PERIOD`, `GLOB_STAR`,
//! `GLOB_NO_DOTDIRS`, `GLOB_TILDE_CHECK`, `GLOB_ONLYDIR` and `GLOB_KEEPSTAT` are not
//! implemented (OI-3) and their bits are ignored.

use alloc::vec::Vec;

use raven_libc_abi::generated::errno::{E2BIG, ENAMETOOLONG, ENOENT, ENOTDIR};
use raven_libc_abi::generated::stat::{S_IFDIR, S_IFLNK, S_IFMT};

use crate::consts::*;
use crate::locale::{MbResult, PatternLocale, encode};
use crate::{DirSys, GlobEnv};

type Char = u64;

const LBRACKET: Char = b'[' as Char;
const NOT: Char = b'!' as Char;
const QUESTION: Char = b'?' as Char;
const QUOTE: Char = b'\\' as Char;
const RANGE: Char = b'-' as Char;
const RBRACKET: Char = b']' as Char;
const SEP: Char = b'/' as Char;
const STAR: Char = b'*' as Char;
const TILDE: Char = b'~' as Char;
const LBRACE: Char = b'{' as Char;
const RBRACE: Char = b'}' as Char;
const COMMA: Char = b',' as Char;

const M_QUOTE: Char = 0x80_0000_0000;
const M_PROTECT: Char = 0x40_0000_0000;
const M_MASK: Char = 0xff_ffff_ffff;
const M_CHAR: Char = 0x0000_ffff_ffff;

const M_ALL: Char = M_QUOTE | STAR;
const M_END: Char = M_QUOTE | RBRACKET;
const M_NOT: Char = M_QUOTE | NOT;
const M_ONE: Char = M_QUOTE | QUESTION;
const M_RNG: Char = M_QUOTE | RANGE;
const M_SET: Char = M_QUOTE | LBRACKET;

/// `CHAR(c)`.
fn ch(c: Char) -> Char {
    c & M_CHAR
}

/// `UNPROT(c)`.
fn unprot(c: Char) -> Char {
    c & !M_PROTECT
}

/// `ismeta(c)`.
fn ismeta(c: Char) -> bool {
    c & M_QUOTE != 0
}

/// The entry at `index`, or the donor's `EOS` zero.
fn at(s: &[Char], index: usize) -> Char {
    s.get(index).copied().unwrap_or(0)
}

/// The byte at `index`, or NUL.
fn atb(s: &[u8], index: usize) -> u8 {
    s.get(index).copied().unwrap_or(0)
}

/// Write at `index`, appending when it is the next slot.
fn put(buf: &mut Vec<Char>, index: usize, c: Char) {
    if index == buf.len() {
        buf.push(c);
    } else if let Some(slot) = buf.get_mut(index) {
        *slot = c;
    }
}

/// `g_strchr` over a `Char` slice.
fn strchr(s: &[Char], c: Char) -> Option<usize> {
    s.iter().position(|&x| x == c)
}

/// The result of one `glob` call.
#[derive(Clone, Debug, Default)]
pub struct GlobState {
    /// The matched paths, in insertion order (the shell maps this to `gl_pathv[gl_offs..]`).
    pub pathv: Vec<Vec<u8>>,
    /// The `gl_offs` count: leading null slots the shell reserves.
    pub offs: usize,
    /// `gl_matchc`: on input with `GLOB_LIMIT`, the path limit; on output, the match count.
    pub matchc: usize,
    /// `gl_flags`: the flags with `GLOB_MAGCHAR` cleared, then set when the pattern had magic.
    pub flags: u32,
    /// The C `errno` to store in the shell; `E2BIG` when a limit was exceeded, else 0.
    ///
    /// This is the crate's replacement for the donor's global `errno` (`errno in shell`,
    /// 05-transpile-guide 3.12); the shell stores it only when it is non-zero.
    pub errno: i32,
}

/// The donor's `struct glob_limit`.
#[derive(Clone, Copy, Default)]
struct Limit {
    brace_cnt: usize,
    path_lim: usize,
    readdir_cnt: usize,
    stat_cnt: usize,
    string_cnt: usize,
}

/// The caller's `gl_errfunc` hook: a path and the error value, returning non-zero to abort.
pub type ErrFunc<'a> = &'a mut dyn FnMut(&[u8], i32) -> i32;

/// The per-call state the donor keeps in globals and in `glob_t`.
struct Ctx<'a> {
    sys: &'a mut dyn DirSys,
    env: &'a dyn GlobEnv,
    loc: &'a dyn PatternLocale,
    errfunc: Option<ErrFunc<'a>>,
    out: &'a mut GlobState,
    limit: Limit,
    errno: i32,
}

impl Ctx<'_> {
    /// A limit failure: `errno = E2BIG`, return `GLOB_NOSPACE`.
    fn too_big(&mut self) -> i32 {
        self.errno = E2BIG;
        GLOB_NOSPACE
    }
}

/// Decode a byte string into `Char`s the donor way: invalid sequences become one literal byte.
fn to_chars(loc: &dyn PatternLocale, bytes: &[u8], out: &mut Vec<Char>) {
    let mut i = 0usize;
    while i < bytes.len() {
        let Some(&first) = bytes.get(i) else {
            break;
        };
        if first == 0 {
            break;
        }
        if first < 128 {
            out.push(Char::from(first));
            i = i.wrapping_add(1);
            continue;
        }
        match loc.mbtowc(bytes.get(i..).unwrap_or(&[])) {
            MbResult::Char { wc, len } => {
                out.push(Char::from(wc));
                i = i.wrapping_add(len);
            }
            MbResult::Incomplete | MbResult::Invalid => {
                out.push(Char::from(first));
                i = i.wrapping_add(1);
            }
        }
    }
}

/// `g_Ctoc`: encode a `Char` path back to bytes, one raw byte for an unencodable value.
fn to_bytes(loc: &dyn PatternLocale, chars: &[Char]) -> Vec<u8> {
    let mut out = Vec::new();
    for &c in chars {
        let wc = ch(c);
        let (bytes, len) = encode(loc, wc as u32);
        if len == 0 {
            out.push((wc & 0xff) as u8);
        } else {
            out.extend_from_slice(bytes.get(..len).unwrap_or(&[]));
        }
    }
    out
}

/// `match()`: the donor's non-recursive last-star matcher (`glob.c:935-1000`).
fn match_name(name: &[Char], pat: &[Char]) -> bool {
    let mut ni = 0usize;
    let mut pi = 0usize;
    let mut nextn: Option<usize> = None;
    let mut nextp: Option<usize> = None;
    loop {
        let mut failed = false;
        while pi < pat.len() {
            let c = at(pat, pi);
            pi = pi.wrapping_add(1);
            match c & M_MASK {
                M_ALL => {
                    if pi == pat.len() {
                        return true;
                    }
                    if name.get(ni).is_none_or(|&x| x == 0) {
                        return false;
                    }
                    nextn = Some(ni.wrapping_add(1));
                    nextp = Some(pi.wrapping_sub(1));
                }
                M_ONE => {
                    let Some(&x) = name.get(ni) else {
                        failed = true;
                        break;
                    };
                    if x == 0 {
                        failed = true;
                        break;
                    }
                    ni = ni.wrapping_add(1);
                }
                M_SET => {
                    let Some(&k) = name.get(ni) else {
                        failed = true;
                        break;
                    };
                    if k == 0 {
                        failed = true;
                        break;
                    }
                    ni = ni.wrapping_add(1);
                    let negate_range = (at(pat, pi) & M_MASK) == M_NOT;
                    if negate_range {
                        pi = pi.wrapping_add(1);
                    }
                    let mut ok = false;
                    while let Some(&set_c) = pat.get(pi) {
                        pi = pi.wrapping_add(1);
                        if set_c & M_MASK == M_END {
                            break;
                        }
                        if (at(pat, pi) & M_MASK) == M_RNG {
                            let lo = ch(set_c);
                            let hi = ch(at(pat, pi.wrapping_add(1)));
                            if lo <= ch(k) && ch(k) <= hi {
                                ok = true;
                            }
                            pi = pi.wrapping_add(2);
                        } else if set_c == k {
                            ok = true;
                        }
                    }
                    if ok == negate_range {
                        failed = true;
                        break;
                    }
                }
                _ => {
                    if name.get(ni).copied() != Some(c) {
                        failed = true;
                        break;
                    }
                    ni = ni.wrapping_add(1);
                }
            }
        }
        if !failed && name.get(ni).is_none_or(|&x| x == 0) {
            return true;
        }
        match (nextn, nextp) {
            (Some(n), Some(p)) => {
                nextn = Some(n);
                ni = n;
                pi = p;
            }
            _ => return false,
        }
    }
}

fn is_dir(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFDIR
}

fn is_link(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFLNK
}

impl Ctx<'_> {
    /// `err_nomatch`.
    fn err_nomatch(&mut self, origpat: &[u8]) -> i32 {
        if self.out.flags & (GLOB_NOCHECK | GLOB_NOMAGIC) != 0
            && (self.out.flags & GLOB_NOCHECK != 0 || self.out.flags & GLOB_MAGCHAR == 0)
        {
            return self.globextend(None, Some(origpat));
        }
        GLOB_NOMATCH
    }

    /// `err_aborted`: call the error hook unless it is NULL; `GLOB_ERR` overrides it.
    fn err_aborted(&mut self, err: i32, buf: &[u8]) -> i32 {
        let mut rv = 0;
        if let Some(func) = self.errfunc.as_mut() {
            rv = func(buf, err);
        }
        if rv != 0 || self.out.flags & GLOB_ERR != 0 {
            return GLOB_ABORTED;
        }
        0
    }

    /// `globextend`.
    fn globextend(&mut self, path: Option<&[Char]>, origpat: Option<&[u8]>) -> i32 {
        if self.out.flags & GLOB_LIMIT != 0 && self.out.matchc > self.limit.path_lim {
            return self.too_big();
        }
        let bytes = match origpat {
            Some(p) => p.to_vec(),
            None => {
                let Some(chars) = path else {
                    self.errno = E2BIG;
                    return GLOB_NOSPACE;
                };
                to_bytes(self.loc, chars)
            }
        };
        self.limit.string_cnt = self
            .limit
            .string_cnt
            .saturating_add(bytes.len().saturating_add(1));
        if self.out.flags & GLOB_LIMIT != 0 && self.limit.string_cnt >= GLOB_LIMIT_STRING {
            return self.too_big();
        }
        self.out.pathv.push(bytes);
        0
    }

    /// `globfinal`.
    fn globfinal(&mut self, oldpathc: usize, origpat: &[u8]) -> i32 {
        if self.out.pathv.len() == oldpathc {
            return self.err_nomatch(origpat);
        }
        if self.out.flags & GLOB_NOSORT == 0
            && let Some(tail) = self.out.pathv.get_mut(oldpathc..)
        {
            tail.sort_unstable();
        }
        0
    }

    /// `globtilde`.
    fn globtilde(&mut self, pattern: &[Char]) -> Result<Vec<Char>, ()> {
        if at(pattern, 0) != TILDE || self.out.flags & GLOB_TILDE == 0 {
            return Ok(pattern.to_vec());
        }
        let mut user = Vec::new();
        let mut p = 1usize;
        while p < pattern.len() && unprot(at(pattern, p)) != SEP {
            if user.len() >= MAXPATHLEN.wrapping_sub(1) {
                return Err(());
            }
            user.push(at(pattern, p));
            p = p.wrapping_add(1);
        }
        if p < pattern.len() && unprot(at(pattern, p)) != SEP {
            return Err(());
        }

        let home: Vec<u8> = if user.is_empty() {
            if let Some(home) = self.env.secure_getenv_home() {
                home
            } else {
                let by_name = self
                    .env
                    .getlogin()
                    .and_then(|login| self.env.getpwnam_dir(&login));
                match by_name.or_else(|| self.env.getpwuid_self_dir()) {
                    Some(home) => home,
                    None => return Ok(pattern.to_vec()),
                }
            }
        } else {
            let name = to_bytes(self.loc, &user);
            match self.env.getpwnam_dir(&name) {
                Some(home) => home,
                None => return Ok(pattern.to_vec()),
            }
        };

        let mut wbuf = Vec::new();
        to_chars(self.loc, &home, &mut wbuf);
        if wbuf.len() > MAXPATHLEN.wrapping_sub(1) {
            return Err(());
        }
        let mut result: Vec<Char> = wbuf.iter().map(|&c| c | M_PROTECT).collect();
        while p < pattern.len() {
            if result.len() >= MAXPATHLEN.wrapping_sub(1) {
                return Err(());
            }
            result.push(at(pattern, p));
            p = p.wrapping_add(1);
        }
        Ok(result)
    }

    /// `glob0`: compile the pattern and match it.
    fn glob0(&mut self, pattern: &[Char], origpat: Option<&[u8]>) -> i32 {
        let qpat = match self.globtilde(pattern) {
            Ok(qpat) => qpat,
            Err(()) => return self.too_big(),
        };
        let oldpathc = self.out.pathv.len();
        let mut compiled: Vec<Char> = Vec::new();
        let mut i = 0usize;
        while i < qpat.len() {
            let c = at(&qpat, i);
            i = i.wrapping_add(1);
            match c {
                LBRACKET => {
                    let mut after = i;
                    let mut first = at(&qpat, after);
                    if first == NOT {
                        after = after.wrapping_add(1);
                    }
                    if at(&qpat, after) == 0
                        || strchr(qpat.get(after.wrapping_add(1)..).unwrap_or(&[]), RBRACKET)
                            .is_none()
                    {
                        compiled.push(LBRACKET);
                        if first == NOT {
                            after = after.wrapping_sub(1);
                        }
                        i = after;
                        continue;
                    }
                    compiled.push(M_SET);
                    if first == NOT {
                        compiled.push(M_NOT);
                    }
                    first = at(&qpat, after);
                    after = after.wrapping_add(1);
                    loop {
                        compiled.push(ch(first));
                        if at(&qpat, after) == RANGE && at(&qpat, after.wrapping_add(1)) != RBRACKET
                        {
                            compiled.push(M_RNG);
                            compiled.push(ch(at(&qpat, after.wrapping_add(1))));
                            after = after.wrapping_add(2);
                        }
                        first = at(&qpat, after);
                        after = after.wrapping_add(1);
                        if first == RBRACKET {
                            break;
                        }
                    }
                    compiled.push(M_END);
                    self.out.flags |= GLOB_MAGCHAR;
                    i = after;
                }
                QUESTION => {
                    self.out.flags |= GLOB_MAGCHAR;
                    compiled.push(M_ONE);
                }
                STAR => {
                    self.out.flags |= GLOB_MAGCHAR;
                    if compiled.last() != Some(&M_ALL) {
                        compiled.push(M_ALL);
                    }
                }
                _ => compiled.push(ch(c)),
            }
        }

        let mut rv = self.glob1(&compiled);
        if rv != 0 {
            return rv;
        }
        if let Some(origpat) = origpat {
            rv = self.globfinal(oldpathc, origpat);
        }
        rv
    }

    /// `glob1`.
    fn glob1(&mut self, pattern: &[Char]) -> i32 {
        if pattern.is_empty() {
            return 0;
        }
        let mut pathbuf: Vec<Char> = Vec::new();
        self.glob2(&mut pathbuf, 0, pattern, 0)
    }

    /// `glob2`: walk the literal segments, recurse at the first segment with a metacharacter.
    fn glob2(
        &mut self,
        pathbuf: &mut Vec<Char>,
        pathend: usize,
        pattern: &[Char],
        pattern_start: usize,
    ) -> i32 {
        let mut anymeta = false;
        let mut pi = pattern_start;
        let mut end = pathend;
        loop {
            if pi == pattern.len() {
                pathbuf.truncate(end);
                let path = to_bytes(self.loc, pathbuf);
                let Ok(sb) = self.sys.lstat(&path) else {
                    return 0;
                };
                if self.out.flags & GLOB_LIMIT != 0 {
                    if self.limit.stat_cnt >= GLOB_LIMIT_STAT {
                        return self.too_big();
                    }
                    self.limit.stat_cnt = self.limit.stat_cnt.wrapping_add(1);
                }
                let last = end.checked_sub(1).and_then(|i| pathbuf.get(i).copied());
                if self.out.flags & GLOB_MARK != 0 && last.is_some_and(|c| unprot(c) != SEP) {
                    let mark = is_dir(sb.st_mode)
                        || (is_link(sb.st_mode)
                            && matches!(self.sys.stat(&path), Ok(s) if is_dir(s.st_mode)));
                    if mark {
                        if end.wrapping_add(1) > MAXPATHLEN.wrapping_sub(1) {
                            return self.too_big();
                        }
                        put(pathbuf, end, SEP);
                        end = end.wrapping_add(1);
                        pathbuf.truncate(end);
                    }
                }
                self.out.matchc = self.out.matchc.wrapping_add(1);
                return self.globextend(Some(pathbuf.get(..end).unwrap_or(&[])), None);
            }

            let mut q = end;
            let mut p = pi;
            while p < pattern.len() && unprot(at(pattern, p)) != SEP {
                if ismeta(at(pattern, p)) {
                    anymeta = true;
                }
                if q.wrapping_add(1) > MAXPATHLEN.wrapping_sub(1) {
                    return self.too_big();
                }
                put(pathbuf, q, at(pattern, p));
                q = q.wrapping_add(1);
                p = p.wrapping_add(1);
            }

            if anymeta {
                return self.glob3(pathbuf, end, pattern, pi, p);
            }
            end = q;
            pi = p;
            while pi < pattern.len() && unprot(at(pattern, pi)) == SEP {
                if end.wrapping_add(1) > MAXPATHLEN.wrapping_sub(1) {
                    return self.too_big();
                }
                put(pathbuf, end, at(pattern, pi));
                end = end.wrapping_add(1);
                pi = pi.wrapping_add(1);
            }
        }
    }

    /// `glob3`: read one directory, match the segment, recurse for each name.
    fn glob3(
        &mut self,
        pathbuf: &mut Vec<Char>,
        pathend: usize,
        pattern: &[Char],
        seg_start: usize,
        seg_end: usize,
    ) -> i32 {
        if pathend > MAXPATHLEN.wrapping_sub(1) {
            return self.too_big();
        }
        pathbuf.truncate(pathend);
        let dirpath = to_bytes(self.loc, pathbuf);
        let mut handle = match self.sys.opendir(&dirpath) {
            Ok(handle) => handle,
            Err(e) => {
                if e.0 == ENOENT as u16 || e.0 == ENOTDIR as u16 {
                    return 0;
                }
                return self.err_aborted(i32::from(e.0), &dirpath);
            }
        };

        let mut err = 0;
        let mut readdir_err = 0i32;
        loop {
            let entry = match self.sys.readdir(&mut handle) {
                Ok(Some(name)) => name,
                Ok(None) => break,
                Err(e) => {
                    readdir_err = i32::from(e.0);
                    break;
                }
            };
            if self.out.flags & GLOB_LIMIT != 0 {
                if self.limit.readdir_cnt >= GLOB_LIMIT_READDIR {
                    self.errno = E2BIG;
                    err = GLOB_NOSPACE;
                    break;
                }
                self.limit.readdir_cnt = self.limit.readdir_cnt.wrapping_add(1);
            }

            if entry.first() == Some(&b'.') && unprot(at(pattern, seg_start)) != b'.' as Char {
                continue;
            }

            let mut name_chars: Vec<Char> = Vec::new();
            to_chars(self.loc, &entry, &mut name_chars);
            let too_long = pathend.wrapping_add(name_chars.len()) >= MAXPATHLEN;
            if too_long {
                let rv = self.err_aborted(ENAMETOOLONG, &dirpath);
                if rv != 0 {
                    self.errno = ENAMETOOLONG;
                    err = rv;
                    break;
                }
                pathbuf.truncate(pathend);
                continue;
            }

            if !match_name(&name_chars, pattern.get(seg_start..seg_end).unwrap_or(&[])) {
                continue;
            }

            for (offset, &c) in name_chars.iter().enumerate() {
                put(pathbuf, pathend.wrapping_add(offset), c);
            }
            let new_end = pathend.wrapping_add(name_chars.len());
            pathbuf.truncate(new_end);
            err = self.glob2(pathbuf, new_end, pattern, seg_end);
            pathbuf.truncate(pathend);
            if err != 0 {
                break;
            }
        }

        self.sys.closedir(handle);
        if err != 0 {
            return err;
        }
        if readdir_err != 0 {
            let rv = self.err_aborted(readdir_err, &dirpath);
            if rv != 0 {
                return rv;
            }
        }
        0
    }

    /// `globexp0`.
    fn globexp0(&mut self, pattern: &[Char], origpat: &[u8]) -> i32 {
        if pattern.len() == 2 && at(pattern, 0) == LBRACE && at(pattern, 1) == RBRACE {
            // RavenBSD: H-GL-1: the brace limit applies even without GLOB_LIMIT.
            if self.limit.brace_cnt >= GLOB_LIMIT_BRACE {
                return self.too_big();
            }
            self.limit.brace_cnt = self.limit.brace_cnt.wrapping_add(1);
            return self.glob0(pattern, Some(origpat));
        }
        let oldpathc = self.out.pathv.len();
        let rv = self.globexp1(pattern);
        if rv != 0 {
            return rv;
        }
        self.globfinal(oldpathc, origpat)
    }

    /// `globexp1`.
    fn globexp1(&mut self, pattern: &[Char]) -> i32 {
        if let Some(ptr) = strchr(pattern, LBRACE) {
            // RavenBSD: H-GL-1: the brace limit applies even without GLOB_LIMIT.
            if self.limit.brace_cnt >= GLOB_LIMIT_BRACE {
                return self.too_big();
            }
            self.limit.brace_cnt = self.limit.brace_cnt.wrapping_add(1);
            return self.globexp2(ptr, pattern);
        }
        self.glob0(pattern, None)
    }

    /// `globexp2`.
    fn globexp2(&mut self, ptr: usize, pattern: &[Char]) -> i32 {
        let prefix = pattern.get(..ptr).unwrap_or(&[]);
        let mut i = 0i32;
        let mut pe = ptr.wrapping_add(1);
        while pe < pattern.len() {
            let c = at(pattern, pe);
            if c == LBRACKET {
                let pm = pe;
                pe = pe.wrapping_add(1);
                while pe < pattern.len() && at(pattern, pe) != RBRACKET {
                    pe = pe.wrapping_add(1);
                }
                if pe >= pattern.len() {
                    pe = pm;
                }
            } else if c == LBRACE {
                i = i.wrapping_add(1);
            } else if c == RBRACE {
                if i == 0 {
                    break;
                }
                i = i.wrapping_sub(1);
            }
            pe = pe.wrapping_add(1);
        }
        if i != 0 || pe >= pattern.len() {
            return self.glob0(pattern, None);
        }

        let mut i = 0i32;
        let mut pl = ptr.wrapping_add(1);
        let mut pm = ptr.wrapping_add(1);
        while pm <= pe {
            let c = at(pattern, pm);
            let mut build = false;
            match c {
                LBRACKET => {
                    let pm1 = pm;
                    pm = pm.wrapping_add(1);
                    while pm < pattern.len() && at(pattern, pm) != RBRACKET {
                        pm = pm.wrapping_add(1);
                    }
                    if pm >= pattern.len() {
                        pm = pm1;
                    }
                }
                LBRACE => i = i.wrapping_add(1),
                RBRACE => {
                    if i > 0 {
                        i = i.wrapping_sub(1);
                    } else {
                        build = true;
                    }
                }
                COMMA if i == 0 => build = true,
                _ => {}
            }
            if build {
                let mut expanded: Vec<Char> = Vec::new();
                expanded.extend_from_slice(prefix);
                expanded.extend_from_slice(pattern.get(pl..pm).unwrap_or(&[]));
                expanded.extend_from_slice(pattern.get(pe.wrapping_add(1)..).unwrap_or(&[]));
                let rv = self.globexp1(&expanded);
                if rv != 0 {
                    return rv;
                }
                pl = pm.wrapping_add(1);
            }
            pm = pm.wrapping_add(1);
        }
        0
    }
}

/// `__glob`: decode the pattern and dispatch to brace or plain globbing.
fn do_glob<'a>(
    sys: &'a mut dyn DirSys,
    env: &'a dyn GlobEnv,
    loc: &'a dyn PatternLocale,
    pattern: &'a [u8],
    errfunc: Option<ErrFunc<'a>>,
    out: &'a mut GlobState,
    flags: u32,
) -> i32 {
    out.flags = flags & !GLOB_MAGCHAR;
    out.errno = 0;
    if out.flags & GLOB_APPEND == 0 {
        out.pathv.clear();
        if out.flags & GLOB_DOOFFS == 0 {
            out.offs = 0;
        }
    }
    let mut limit = Limit::default();
    if out.flags & GLOB_LIMIT != 0 {
        limit.path_lim = out.matchc;
        if limit.path_lim == 0 {
            limit.path_lim = GLOB_LIMIT_PATH;
        }
    }
    out.matchc = 0;

    let mut ctx = Ctx {
        sys,
        env,
        loc,
        errfunc,
        out,
        limit,
        errno: 0,
    };

    let mut patbuf: Vec<Char> = Vec::new();
    let mut i = 0usize;
    let mut too_long = true;
    if flags & GLOB_NOESCAPE != 0 {
        while patbuf.len() < MAXPATHLEN {
            let Some(&first) = pattern.get(i) else {
                too_long = false;
                break;
            };
            if first == 0 {
                too_long = false;
                break;
            }
            match loc.mbtowc(pattern.get(i..).unwrap_or(&[])) {
                MbResult::Char { wc, len } => {
                    patbuf.push(Char::from(wc));
                    if len == 0 {
                        too_long = false;
                        break;
                    }
                    i = i.wrapping_add(len);
                }
                MbResult::Incomplete | MbResult::Invalid => {
                    let rv = ctx.err_nomatch(pattern);
                    ctx.out.errno = ctx.errno;
                    return rv;
                }
            }
        }
    } else {
        while patbuf.len() < MAXPATHLEN {
            let Some(&first) = pattern.get(i) else {
                too_long = false;
                break;
            };
            if first == 0 {
                too_long = false;
                break;
            }
            let mut prot = 0;
            if first == b'\\' {
                i = i.wrapping_add(1);
                if atb(pattern, i) == 0 {
                    patbuf.push(QUOTE);
                    continue;
                }
                prot = M_PROTECT;
            }
            match loc.mbtowc(pattern.get(i..).unwrap_or(&[])) {
                MbResult::Char { wc, len } => {
                    patbuf.push(Char::from(wc) | prot);
                    if len == 0 {
                        too_long = false;
                        break;
                    }
                    i = i.wrapping_add(len);
                }
                MbResult::Incomplete | MbResult::Invalid => {
                    let rv = ctx.err_nomatch(pattern);
                    ctx.out.errno = ctx.errno;
                    return rv;
                }
            }
        }
    }
    if too_long {
        let rv = ctx.err_nomatch(pattern);
        ctx.out.errno = ctx.errno;
        return rv;
    }

    let rv = if flags & GLOB_BRACE != 0 {
        ctx.globexp0(&patbuf, pattern)
    } else {
        ctx.glob0(&patbuf, Some(pattern))
    };
    ctx.out.errno = ctx.errno;
    rv
}

/// Expand `pattern`; returns `0`, [`GLOB_NOSPACE`], [`GLOB_ABORTED`] or [`GLOB_NOMATCH`].
///
/// `out.matchc` is an *input* path limit when `GLOB_LIMIT` is set and it is non-zero
/// (`A-glob-1`); otherwise it is the output match count. `out.errno` receives `E2BIG` when a
/// limit stopped the expansion.
pub fn glob<'a>(
    sys: &'a mut dyn DirSys,
    env: &'a dyn GlobEnv,
    loc: &'a dyn PatternLocale,
    pattern: &'a [u8],
    flags: u32,
    errfunc: Option<ErrFunc<'a>>,
    out: &'a mut GlobState,
) -> i32 {
    do_glob(sys, env, loc, pattern, errfunc, out, flags)
}

/// Release the results; safe to call twice.
pub fn globfree(out: &mut GlobState) {
    out.pathv.clear();
    out.pathv.shrink_to_fit();
}

#[cfg(test)]
#[path = "glob_ut.rs"]
mod tests;
