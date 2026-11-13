// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/vfscanf.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// The donor file has no per-file header; it is covered by musl/COPYRIGHT (lines 1-24).
// MIT via musl/COPYRIGHT.
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
//! The `scanf` engine (`vscan`).
//!
//! `FILE *f` became `&mut impl ScanSource`, `shgetc`/`shunget`/`shlim`/`shcnt` became
//! [`Limited`], destination stores became [`FmtEnv`] calls and the `%m` allocation became
//! [`FmtEnv::alloc_store`]. The positional branch (`%1$d`) is deleted (RD-36): `%1$d` is a
//! format failure. `%c` does not skip whitespace, `%[` handles sets and ranges, `%n` stores
//! the consumed count and does not count as an assignment, and the return value follows
//! musl exactly (RD-new 9).

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
#![allow(
    clippy::while_let_loop,
    clippy::collapsible_if,
    clippy::needless_range_loop
)]
// The scan loop mirrors the donor's cursors; every input read is bounded by `Limited` and
// every buffer index by a fixed-size array. The loop shares state across several labelled
// breaks, so it is not a plain `while let`; the set initialiser is the donor's two-step
// assignment.

pub mod float_lex;
pub mod int;
pub mod source;

use raven_libc_ctype::{MbMode, MbState, Mbr, mbrtowc, wcrtomb};

use crate::traits::{CharUnit, FloatScan, FmtEnv};
use crate::types::{FloatKind, FmtErr, SCAN_TOKEN_MAX, ScanOptions, ScanResult, StoreSize};
pub use source::{Limited, ScanSource};

/// Marker for the floating-point scan environment (see [`FloatScan`]).
pub trait ScanEnvFloat {}

/// The maximum number of wide characters collected for one `%m` allocation.
const ALLOC_WCHARS: usize = SCAN_TOKEN_MAX / 4;

fn is_space(c: u32) -> bool {
    matches!(c, 0x20 | 0x09 | 0x0a | 0x0b | 0x0c | 0x0d)
}

fn is_digit(c: u32) -> bool {
    (0x30..=0x39).contains(&c)
}

fn member(set: &[bool; 257], c: u32) -> bool {
    if c > 255 {
        false
    } else {
        set.get((c as usize).wrapping_add(1))
            .copied()
            .unwrap_or(false)
    }
}

fn set_byte(set: &mut [bool; 257], c: u32, value: bool) {
    if c <= 255 {
        if let Some(slot) = set.get_mut((c as usize).wrapping_add(1)) {
            *slot = value;
        }
    }
}

enum Fail {
    Input,
    Match,
    Format,
}

enum FailResult {
    Match,
    Format,
    Alloc,
    Encoding,
}

struct Scanner<'a, C: CharUnit, S: ScanSource<C>, A: FmtEnv, F: FloatScan> {
    f: Limited<'a, S, C>,
    args: &'a mut A,
    floats: &'a F,
    opts: &'a ScanOptions<'a>,
    pos: usize,
    assigned: i32,
}

impl<'a, C: CharUnit, S: ScanSource<C>, A: FmtEnv, F: FloatScan> Scanner<'a, C, S, A, F> {
    fn settle(&mut self) {
        self.pos = self.pos.saturating_add(self.f.count());
        self.f.shlim(0);
    }

    fn fail(&mut self, kind: Fail) -> ScanResult {
        self.settle();
        self.f.finish();
        let mut return_eof = false;
        match kind {
            Fail::Input | Fail::Format => {
                if self.assigned == 0 {
                    return_eof = true;
                }
            }
            Fail::Match => {}
        }
        ScanResult {
            assigned: self.assigned,
            consumed: self.pos,
            return_eof,
        }
    }

    fn skip_space(&mut self) {
        self.f.shlim(0);
        while let Some(c) = self.f.getc() {
            if !is_space(c.to_u32()) {
                self.f.ungetc(c);
                break;
            }
        }
    }
}

/// The `%[` set after parsing, plus the format index after `]`.
struct ScanSet {
    set: [bool; 257],
}

/// Parse the `%[` set starting at `p` (which points just after `[`); returns the set and
/// the index after the closing `]`.
fn parse_set<C: CharUnit>(fmt: &[C], mut p: usize) -> Result<(ScanSet, usize), FailResult> {
    let mut set = [false; 257];
    let mut invert = false;
    if fmt.get(p).map(|c| c.to_u32()) == Some(u32::from(b'^')) {
        invert = true;
        p = p.wrapping_add(1);
    }
    if fmt.get(p).map(|c| c.to_u32()) == Some(u32::from(b'-')) {
        set_byte(&mut set, u32::from(b'-'), true);
        p = p.wrapping_add(1);
    } else if fmt.get(p).map(|c| c.to_u32()) == Some(u32::from(b']')) {
        set_byte(&mut set, u32::from(b']'), true);
        p = p.wrapping_add(1);
    }
    loop {
        let Some(&ch) = fmt.get(p) else {
            return Err(FailResult::Format);
        };
        let cv = ch.to_u32();
        if cv == 0 {
            return Err(FailResult::Format);
        }
        if cv == u32::from(b']') {
            p = p.wrapping_add(1);
            break;
        }
        if cv == u32::from(b'-')
            && fmt.get(p.wrapping_add(1)).is_some()
            && fmt.get(p.wrapping_add(1)).map(|c| c.to_u32()) != Some(u32::from(b']'))
        {
            let prev = fmt.get(p.wrapping_sub(1)).map(|c| c.to_u32()).unwrap_or(0);
            let next = fmt.get(p.wrapping_add(1)).map(|c| c.to_u32()).unwrap_or(0);
            let mut c = prev;
            while c < next {
                set_byte(&mut set, c, true);
                c = c.wrapping_add(1);
            }
            set_byte(&mut set, next, true);
            p = p.wrapping_add(2);
            continue;
        }
        set_byte(&mut set, cv, true);
        p = p.wrapping_add(1);
    }
    if invert {
        for slot in set.iter_mut().skip(1) {
            *slot = !*slot;
        }
    }
    Ok((ScanSet { set }, p))
}

fn set_for(t: u8) -> ScanSet {
    let mut set = [false; 257];
    match t {
        b's' => {
            for slot in set.iter_mut() {
                *slot = true;
            }
            set_byte(&mut set, 0, false);
            for c in [0x09u32, 0x0a, 0x0b, 0x0c, 0x0d, 0x20] {
                set_byte(&mut set, c, false);
            }
        }
        _ => {
            // %c
            for slot in set.iter_mut() {
                *slot = true;
            }
            set_byte(&mut set, 0, false);
        }
    }
    ScanSet { set }
}

/// Run one `scanf` call over `src`.
///
/// # Errors
/// [`FmtErr::NoMem`] for a failed `%m` allocation and [`FmtErr::Encoding`] for invalid
/// multibyte input. A bad directive is a matching failure, not an error.
pub fn vscan<C: CharUnit, S: ScanSource<C>, A: FmtEnv, F: FloatScan>(
    src: &mut S,
    fmt: &[C],
    args: &mut A,
    opts: &ScanOptions<'_>,
    floats: &F,
) -> Result<ScanResult, FmtErr> {
    let mut s = Scanner {
        f: Limited::new(src),
        args,
        floats,
        opts,
        pos: 0,
        assigned: 0,
    };
    let mut p = 0usize;
    loop {
        let Some(&fch) = fmt.get(p) else {
            break;
        };
        let ch = fch.to_u32();
        if ch == 0 {
            break;
        }
        if is_space(ch) {
            while fmt
                .get(p.wrapping_add(1))
                .map(|c| is_space(c.to_u32()))
                .unwrap_or(false)
            {
                p = p.wrapping_add(1);
            }
            s.skip_space();
            s.settle();
            p = p.wrapping_add(1);
            continue;
        }
        if ch != u32::from(b'%')
            || fmt.get(p.wrapping_add(1)).map(|c| c.to_u32()).unwrap_or(0) == u32::from(b'%')
        {
            s.f.shlim(0);
            let literal;
            if ch == u32::from(b'%') {
                p = p.wrapping_add(1);
                while let Some(c) = s.f.getc() {
                    if !is_space(c.to_u32()) {
                        s.f.ungetc(c);
                        break;
                    }
                }
                literal = u32::from(b'%');
            } else {
                literal = ch;
            }
            let c = s.f.getc();
            let same = c.map(|v| v.to_u32() == literal).unwrap_or(false);
            if !same {
                if let Some(v) = c {
                    s.f.ungetc(v);
                }
                if c.is_none() {
                    return Ok(s.fail(Fail::Input));
                }
                return Ok(s.fail(Fail::Match));
            }
            s.settle();
            p = p.wrapping_add(1);
            continue;
        }

        p = p.wrapping_add(1);
        let mut dest: usize = 0;
        let mut dest_present = false;
        let next = fmt.get(p).map(|c| c.to_u32()).unwrap_or(0);
        if next == u32::from(b'*') {
            p = p.wrapping_add(1);
        } else if is_digit(next)
            && fmt.get(p.wrapping_add(1)).map(|c| c.to_u32()) == Some(u32::from(b'$'))
        {
            return Ok(s.fail(Fail::Format));
        } else {
            dest = s.args.next_u64() as usize;
            dest_present = true;
        }

        let mut width = 0usize;
        while let Some(c) = fmt.get(p).map(|c| c.to_u32()) {
            if !is_digit(c) {
                break;
            }
            width = width
                .saturating_mul(10)
                .saturating_add(c.wrapping_sub(0x30) as usize);
            p = p.wrapping_add(1);
        }
        let mut alloc = false;
        if fmt.get(p).map(|c| c.to_u32()) == Some(u32::from(b'm')) {
            alloc = dest_present;
            p = p.wrapping_add(1);
        }

        let mut size = StoreSize::Int;
        let mut long_double = false;
        if let Some(&lch) = fmt.get(p) {
            match lch.to_u32() as u8 {
                b'h' => {
                    p = p.wrapping_add(1);
                    if fmt.get(p).map(|c| c.to_u32()) == Some(u32::from(b'h')) {
                        size = StoreSize::Hh;
                        p = p.wrapping_add(1);
                    } else {
                        size = StoreSize::H;
                    }
                }
                b'l' => {
                    p = p.wrapping_add(1);
                    if fmt.get(p).map(|c| c.to_u32()) == Some(u32::from(b'l')) {
                        size = StoreSize::Ll;
                        p = p.wrapping_add(1);
                    } else {
                        size = StoreSize::L;
                    }
                }
                b'j' => {
                    size = StoreSize::J;
                    p = p.wrapping_add(1);
                }
                b'z' | b't' => {
                    size = StoreSize::L;
                    p = p.wrapping_add(1);
                }
                b'L' => {
                    long_double = true;
                    size = StoreSize::L;
                    p = p.wrapping_add(1);
                }
                _ => {}
            }
        }
        let Some(&tch) = fmt.get(p) else {
            return Ok(s.fail(Fail::Format));
        };
        let mut t = tch.to_u32();
        if (t & 0x2f) == 3 {
            t |= 32;
            size = StoreSize::L;
        }
        p = p.wrapping_add(1);

        let set;
        match t as u8 {
            b'n' => {
                if dest_present && dest != 0 {
                    s.args.store_int(dest, size, s.pos as u64);
                }
                continue;
            }
            b'c' => {
                if width < 1 {
                    width = 1;
                }
                set = set_for(b'c');
            }
            b'[' => {
                let (parsed, next_p) = match parse_set(fmt, p) {
                    Ok(v) => v,
                    Err(_) => return Ok(s.fail(Fail::Format)),
                };
                set = parsed;
                p = next_p;
            }
            b's' => {
                set = set_for(b's');
                s.skip_space();
                s.settle();
            }
            b'p' | b'X' | b'x' | b'o' | b'd' | b'u' | b'i' | b'a' | b'A' | b'e' | b'E' | b'f'
            | b'F' | b'g' | b'G' => {
                set = set_for(b's');
                s.skip_space();
                s.settle();
            }
            _ => return Ok(s.fail(Fail::Format)),
        }

        s.f.shlim(width);
        match s.f.getc() {
            Some(c) => s.f.ungetc(c),
            None => return Ok(s.fail(Fail::Input)),
        }

        let result = match t as u8 {
            b'c' | b's' | b'[' => scan_string::<C, S, A, F>(
                &mut s,
                &set.set,
                t as u8,
                size,
                width,
                dest,
                dest_present,
                alloc,
            ),
            b'p' | b'X' | b'x' => scan_int(&mut s, 16, size, dest, dest_present, t as u8 == b'p'),
            b'o' => scan_int(&mut s, 8, size, dest, dest_present, false),
            b'd' | b'u' => scan_int(&mut s, 10, size, dest, dest_present, false),
            b'i' => scan_int(&mut s, 0, size, dest, dest_present, false),
            _ => {
                let kind = if long_double {
                    FloatKind::LongDouble
                } else if size == StoreSize::L {
                    FloatKind::F64
                } else {
                    FloatKind::F32
                };
                scan_float(&mut s, kind, dest, dest_present)
            }
        };
        match result {
            Ok(()) => {
                if dest_present {
                    s.assigned = s.assigned.saturating_add(1);
                }
                s.settle();
            }
            Err(FailResult::Match) => return Ok(s.fail(Fail::Match)),
            Err(FailResult::Format) => return Ok(s.fail(Fail::Format)),
            Err(FailResult::Alloc) => return Err(FmtErr::NoMem),
            Err(FailResult::Encoding) => return Err(FmtErr::Encoding),
        }
    }
    s.settle();
    s.f.finish();
    Ok(ScanResult {
        assigned: s.assigned,
        consumed: s.pos,
        return_eof: false,
    })
}

fn scan_int<C: CharUnit, S: ScanSource<C>, A: FmtEnv, F: FloatScan>(
    s: &mut Scanner<'_, C, S, A, F>,
    base: u32,
    size: StoreSize,
    dest: usize,
    dest_present: bool,
    pointer: bool,
) -> Result<(), FailResult> {
    let value = int::intscan(&mut s.f, base, false, u64::MAX);
    if s.f.count() == 0 {
        return Err(FailResult::Match);
    }
    if dest_present && dest != 0 {
        if pointer {
            s.args.store_int(dest, StoreSize::L, value);
        } else {
            s.args.store_int(dest, size, value);
        }
    }
    Ok(())
}

#[inline(never)]
fn scan_float<C: CharUnit, S: ScanSource<C>, A: FmtEnv, F: FloatScan>(
    s: &mut Scanner<'_, C, S, A, F>,
    kind: FloatKind,
    dest: usize,
    dest_present: bool,
) -> Result<(), FailResult> {
    let mut buf = [0u8; SCAN_TOKEN_MAX];
    let len = float_lex::parse_token(&mut s.f, &mut buf);
    if len == 0 {
        return Err(FailResult::Match);
    }
    let token = buf.get(..len).unwrap_or(&[]);
    match s.floats.parse(token, kind) {
        Ok(bits) => {
            if dest_present && dest != 0 {
                s.args.store_float_bits(dest, kind, bits.bits);
            }
            Ok(())
        }
        Err(FmtErr::NoMem) => Err(FailResult::Alloc),
        Err(_) => Err(FailResult::Match),
    }
}

fn encode_wide(mode: MbMode, wc: u32, out: &mut [u8; 4]) -> Result<usize, FailResult> {
    wcrtomb(mode, wc, out).map_err(|_| FailResult::Encoding)
}

/// The `%c`, `%s` and `%[` conversions.
#[allow(clippy::too_many_arguments)] // mirrors the donor's conversion state list
fn scan_string<C: CharUnit, S: ScanSource<C>, A: FmtEnv, F: FloatScan>(
    s: &mut Scanner<'_, C, S, A, F>,
    set: &[bool; 257],
    t: u8,
    size: StoreSize,
    width: usize,
    dest: usize,
    dest_present: bool,
    alloc: bool,
) -> Result<(), FailResult> {
    let wide_dest = size == StoreSize::L && (t == b'c' || t == b's' || t == b'[');
    let limit = if width == 0 { usize::MAX } else { width };
    let mut chars = 0usize;
    let mut byte_off = 0usize;
    let mut wchar_off = 0usize;
    let mut byte_buf = [0u8; 256];
    let mut byte_len = 0usize;
    let mut wchar_buf = [0u32; 128];
    let mut wchar_len = 0usize;
    let mut alloc_bytes = [0u8; SCAN_TOKEN_MAX];
    let mut alloc_len = 0usize;
    let mut alloc_wchars = [0u32; ALLOC_WCHARS];
    let mut alloc_wlen = 0usize;
    let mut state = MbState::INITIAL;
    let mut pending_bytes = 0usize;

    loop {
        if chars >= limit {
            break;
        }
        let Some(c) = s.f.getc() else {
            break;
        };
        let cv = c.to_u32();
        let ok = if t == b'c' {
            cv != 0
        } else if t == b's' {
            cv != 0 && !is_space(cv)
        } else {
            member(set, cv)
        };
        if !ok {
            s.f.ungetc(c);
            break;
        }

        if wide_dest {
            let wc;
            if C::WIDE {
                wc = cv;
            } else {
                let b = cv as u8;
                match mbrtowc(&mut state, s.opts.mode, &[b]) {
                    Mbr::Char { wc: v, used } => {
                        let _ = used;
                        wc = v;
                        if pending_bytes > 0 {
                            pending_bytes = 0;
                        }
                    }
                    Mbr::Incomplete => {
                        pending_bytes = pending_bytes.saturating_add(1);
                        continue;
                    }
                    Mbr::Ilseq => return Err(FailResult::Encoding),
                }
            }
            chars = chars.saturating_add(1);
            if alloc {
                if alloc_wlen < alloc_wchars.len() {
                    alloc_wchars[alloc_wlen] = wc;
                    alloc_wlen = alloc_wlen.saturating_add(1);
                }
            } else if dest_present && dest != 0 {
                if wchar_len < wchar_buf.len() {
                    wchar_buf[wchar_len] = wc;
                    wchar_len = wchar_len.saturating_add(1);
                }
                if wchar_len == wchar_buf.len() {
                    s.args
                        .store_wchars(dest.wrapping_add(wchar_off.saturating_mul(4)), &wchar_buf);
                    wchar_off = wchar_off.saturating_add(wchar_len);
                    wchar_len = 0;
                }
            }
        } else {
            chars = chars.saturating_add(1);
            let mut tmp = [0u8; 4];
            let bytes: &[u8] = if C::WIDE {
                let k = encode_wide(s.opts.mode, cv, &mut tmp)?;
                tmp.get(..k).unwrap_or(&[])
            } else {
                tmp[0] = cv as u8;
                tmp.get(..1).unwrap_or(&[])
            };
            for &b in bytes {
                if alloc {
                    if alloc_len < alloc_bytes.len() {
                        alloc_bytes[alloc_len] = b;
                        alloc_len = alloc_len.saturating_add(1);
                    }
                } else if dest_present && dest != 0 {
                    if byte_len < byte_buf.len() {
                        byte_buf[byte_len] = b;
                        byte_len = byte_len.saturating_add(1);
                    }
                    if byte_len == byte_buf.len() {
                        s.args.store_chars(dest.wrapping_add(byte_off), &byte_buf);
                        byte_off = byte_off.saturating_add(byte_len);
                        byte_len = 0;
                    }
                }
            }
        }
    }
    if pending_bytes > 0 {
        return Err(FailResult::Encoding);
    }
    if !alloc && dest_present && dest != 0 {
        if byte_len > 0 {
            s.args.store_chars(
                dest.wrapping_add(byte_off),
                byte_buf.get(..byte_len).unwrap_or(&[]),
            );
            byte_off = byte_off.saturating_add(byte_len);
        }
        if wchar_len > 0 {
            s.args.store_wchars(
                dest.wrapping_add(wchar_off.saturating_mul(4)),
                wchar_buf.get(..wchar_len).unwrap_or(&[]),
            );
            wchar_off = wchar_off.saturating_add(wchar_len);
        }
    }

    if t == b'c' {
        if chars != width {
            return Err(FailResult::Match);
        }
        if alloc {
            let (bytes, n, wide) = if wide_dest {
                let mut image = [0u8; SCAN_TOKEN_MAX];
                let mut n = 0usize;
                for &v in alloc_wchars.get(..alloc_wlen).unwrap_or(&[]) {
                    for &b in v.to_le_bytes().iter() {
                        if n < image.len() {
                            image[n] = b;
                            n = n.saturating_add(1);
                        }
                    }
                }
                (image, n, true)
            } else {
                let mut image = [0u8; SCAN_TOKEN_MAX];
                let n = alloc_len;
                for (dst, &b) in image
                    .iter_mut()
                    .zip(alloc_bytes.get(..alloc_len).unwrap_or(&[]))
                {
                    *dst = b;
                }
                (image, n, false)
            };
            if !s
                .args
                .alloc_store(dest, bytes.get(..n).unwrap_or(&[]), wide)
            {
                return Err(FailResult::Alloc);
            }
        }
        return Ok(());
    }
    if chars == 0 {
        return Err(FailResult::Match);
    }
    if alloc {
        if wide_dest {
            let mut image = [0u8; SCAN_TOKEN_MAX];
            let mut n = 0usize;
            for &v in alloc_wchars.get(..alloc_wlen).unwrap_or(&[]) {
                for &b in v.to_le_bytes().iter() {
                    if n < image.len() {
                        image[n] = b;
                        n = n.saturating_add(1);
                    }
                }
            }
            if !s
                .args
                .alloc_store(dest, image.get(..n).unwrap_or(&[]), true)
            {
                return Err(FailResult::Alloc);
            }
        } else if !s
            .args
            .alloc_store(dest, alloc_bytes.get(..alloc_len).unwrap_or(&[]), false)
        {
            return Err(FailResult::Alloc);
        }
    } else if dest_present && dest != 0 {
        if wide_dest {
            let nul = [0u32];
            s.args
                .store_wchars(dest.wrapping_add(wchar_off.saturating_mul(4)), &nul);
        } else {
            let nul = [0u8];
            s.args.store_chars(dest.wrapping_add(byte_off), &nul);
        }
    }
    Ok(())
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
#[path = "mod_ut.rs"]
mod tests;
