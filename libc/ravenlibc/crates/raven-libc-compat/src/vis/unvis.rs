// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1989, 1993 The Regents of the University of California.  All rights reserved.
//
// RavenBSD: transpiled from netbsd:lib/libc/gen/unvis.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: unvis.c,v 1.47 2026/03/22 20:18:15 kre Exp $)
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
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `unvis`, `strunvis`, `strnunvis`, `strunvisx`, `strnunvisx`: the donor state machine.
//!
//! The state and the RFC 1866 name index live in [`Unvis`]; the C version packs both into one
//! `int` state word and reuses the output byte as the table index. The observable steps are the
//! [`UnvisStep`] variants. `_DIAGASSERT` and `__weak_alias` are not translated.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // direct transliteration: the indices are bounded by the donor algorithm
use super::flags::*;

/// One step of the `unvis` state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnvisStep {
    /// A decoded byte.
    Valid(u8),
    /// A decoded byte; feed the current input byte again.
    ValidPush(u8),
    /// The byte was consumed with no output.
    NoChar,
    /// A bad sequence (the state machine reset itself).
    SynBad,
    /// An output-space error (not produced by [`Unvis`]; the string functions use it).
    Error,
}

/// The RFC 1866 name table of `unvis.c` (`nv[]`), in the donor order.
const NV: &[(&[u8], u8)] = &[
    (b"AElig", 198),
    (b"Aacute", 193),
    (b"Acirc", 194),
    (b"Agrave", 192),
    (b"Aring", 197),
    (b"Atilde", 195),
    (b"Auml", 196),
    (b"Ccedil", 199),
    (b"ETH", 208),
    (b"Eacute", 201),
    (b"Ecirc", 202),
    (b"Egrave", 200),
    (b"Euml", 203),
    (b"Iacute", 205),
    (b"Icirc", 206),
    (b"Igrave", 204),
    (b"Iuml", 207),
    (b"Ntilde", 209),
    (b"Oacute", 211),
    (b"Ocirc", 212),
    (b"Ograve", 210),
    (b"Oslash", 216),
    (b"Otilde", 213),
    (b"Ouml", 214),
    (b"THORN", 222),
    (b"Uacute", 218),
    (b"Ucirc", 219),
    (b"Ugrave", 217),
    (b"Uuml", 220),
    (b"Yacute", 221),
    (b"aacute", 225),
    (b"acirc", 226),
    (b"acute", 180),
    (b"aelig", 230),
    (b"agrave", 224),
    (b"amp", 38),
    (b"aring", 229),
    (b"atilde", 227),
    (b"auml", 228),
    (b"brvbar", 166),
    (b"ccedil", 231),
    (b"cedil", 184),
    (b"cent", 162),
    (b"copy", 169),
    (b"curren", 164),
    (b"deg", 176),
    (b"divide", 247),
    (b"eacute", 233),
    (b"ecirc", 234),
    (b"egrave", 232),
    (b"eth", 240),
    (b"euml", 235),
    (b"frac12", 189),
    (b"frac14", 188),
    (b"frac34", 190),
    (b"gt", 62),
    (b"iacute", 237),
    (b"icirc", 238),
    (b"iexcl", 161),
    (b"igrave", 236),
    (b"iquest", 191),
    (b"iuml", 239),
    (b"laquo", 171),
    (b"lt", 60),
    (b"macr", 175),
    (b"micro", 181),
    (b"middot", 183),
    (b"nbsp", 160),
    (b"not", 172),
    (b"ntilde", 241),
    (b"oacute", 243),
    (b"ocirc", 244),
    (b"ograve", 242),
    (b"ordf", 170),
    (b"ordm", 186),
    (b"oslash", 248),
    (b"otilde", 245),
    (b"ouml", 246),
    (b"para", 182),
    (b"plusmn", 177),
    (b"pound", 163),
    (b"quot", 34),
    (b"raquo", 187),
    (b"reg", 174),
    (b"sect", 167),
    (b"shy", 173),
    (b"sup1", 185),
    (b"sup2", 178),
    (b"sup3", 179),
    (b"szlig", 223),
    (b"thorn", 254),
    (b"times", 215),
    (b"uacute", 250),
    (b"ucirc", 251),
    (b"ugrave", 249),
    (b"uml", 168),
    (b"uuml", 252),
    (b"yacute", 253),
    (b"yen", 165),
    (b"yuml", 255),
];

/// The donor's `S_*` states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Ground = 0,
    Start = 1,
    Meta = 2,
    Meta1 = 3,
    Ctrl = 4,
    Octal2 = 5,
    Octal3 = 6,
    Hex = 7,
    Hex1 = 8,
    Hex2 = 9,
    Mime1 = 10,
    Mime2 = 11,
    EatCrnl = 12,
    Amp = 13,
    Number = 14,
    String = 15,
}

fn state_from(value: u8) -> State {
    match value {
        1 => State::Start,
        2 => State::Meta,
        3 => State::Meta1,
        4 => State::Ctrl,
        5 => State::Octal2,
        6 => State::Octal3,
        7 => State::Hex,
        8 => State::Hex1,
        9 => State::Hex2,
        10 => State::Mime1,
        11 => State::Mime2,
        12 => State::EatCrnl,
        13 => State::Amp,
        14 => State::Number,
        15 => State::String,
        _ => State::Ground,
    }
}

fn is_octal(c: u8) -> bool {
    c.is_ascii_digit() && c <= b'7'
}

fn is_hex(c: u8) -> bool {
    c.is_ascii_hexdigit()
}

fn xtod(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c.wrapping_sub(b'0'),
        b'a'..=b'f' => c.wrapping_sub(b'a').wrapping_add(10),
        _ => c.wrapping_sub(b'A').wrapping_add(10),
    }
}

/// The `unvis` state machine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Unvis {
    state: u8,
    /// Index into [`NV`] (the donor's top state byte).
    nv_index: u8,
    /// The partial output byte (the donor's `*cp`).
    cp: u8,
}

impl Unvis {
    /// A fresh state machine.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one input byte; returns the donor's `unvis()` result as a step.
    pub fn feed(&mut self, c: u8, flags: u32) -> UnvisStep {
        let state = state_from(self.state);
        if flags & UNVIS_END != 0 {
            return self.finish();
        }
        match state {
            State::Ground => {
                self.cp = 0;
                if flags & VIS_NOESCAPE == 0 && c == b'\\' {
                    self.state = State::Start as u8;
                    return UnvisStep::NoChar;
                }
                if flags & VIS_HTTP1808 != 0 && c == b'%' {
                    self.state = State::Hex1 as u8;
                    return UnvisStep::NoChar;
                }
                if flags & VIS_HTTP1866 != 0 && c == b'&' {
                    self.state = State::Amp as u8;
                    return UnvisStep::NoChar;
                }
                if flags & VIS_MIMESTYLE != 0 && c == b'=' {
                    self.state = State::Mime1 as u8;
                    return UnvisStep::NoChar;
                }
                self.cp = c;
                UnvisStep::Valid(c)
            }
            State::Start => self.start(c),
            State::Meta => {
                if c == b'-' {
                    self.state = State::Meta1 as u8;
                } else if c == b'^' {
                    self.state = State::Ctrl as u8;
                } else {
                    return self.bad();
                }
                UnvisStep::NoChar
            }
            State::Meta1 => {
                self.state = State::Ground as u8;
                self.cp |= c;
                UnvisStep::Valid(self.cp)
            }
            State::Ctrl => {
                if c == b'?' {
                    self.cp |= 0o177;
                } else {
                    self.cp |= c & 0o037;
                }
                self.state = State::Ground as u8;
                UnvisStep::Valid(self.cp)
            }
            State::Octal2 => {
                if is_octal(c) {
                    self.cp = self.cp.wrapping_shl(3).wrapping_add(c.wrapping_sub(b'0'));
                    self.state = State::Octal3 as u8;
                    UnvisStep::NoChar
                } else {
                    self.state = State::Ground as u8;
                    UnvisStep::ValidPush(self.cp)
                }
            }
            State::Octal3 => {
                self.state = State::Ground as u8;
                if is_octal(c) {
                    if self.cp & 0o040 != 0 {
                        return self.bad();
                    }
                    self.cp = self.cp.wrapping_shl(3).wrapping_add(c.wrapping_sub(b'0'));
                    UnvisStep::Valid(self.cp)
                } else {
                    UnvisStep::ValidPush(self.cp)
                }
            }
            State::Hex | State::Hex1 => {
                if !is_hex(c) {
                    if state == State::Hex {
                        return self.bad();
                    }
                    self.state = State::Ground as u8;
                    return UnvisStep::ValidPush(self.cp);
                }
                self.cp = xtod(c);
                self.state = State::Hex2 as u8;
                UnvisStep::NoChar
            }
            State::Hex2 => {
                self.state = State::Ground as u8;
                if is_hex(c) {
                    self.cp = xtod(c) | self.cp.wrapping_shl(4);
                    UnvisStep::Valid(self.cp)
                } else {
                    UnvisStep::ValidPush(self.cp)
                }
            }
            State::Mime1 => {
                if c == b'\n' || c == b'\r' {
                    self.state = State::EatCrnl as u8;
                    return UnvisStep::NoChar;
                }
                if is_hex(c) && (c.is_ascii_digit() || c.is_ascii_uppercase()) {
                    self.cp = xtod(c);
                    self.state = State::Mime2 as u8;
                    UnvisStep::NoChar
                } else {
                    self.bad()
                }
            }
            State::Mime2 => {
                if is_hex(c) && (c.is_ascii_digit() || c.is_ascii_uppercase()) {
                    self.state = State::Ground as u8;
                    self.cp = xtod(c) | self.cp.wrapping_shl(4);
                    UnvisStep::Valid(self.cp)
                } else {
                    self.bad()
                }
            }
            State::EatCrnl => match c {
                b'\r' | b'\n' => UnvisStep::NoChar,
                b'=' => {
                    self.state = State::Mime1 as u8;
                    UnvisStep::NoChar
                }
                _ => {
                    self.cp = c;
                    self.state = State::Ground as u8;
                    UnvisStep::Valid(c)
                }
            },
            State::Amp => self.amp(c),
            State::String => self.string(c),
            State::Number => self.number(c),
        }
    }

    fn start(&mut self, c: u8) -> UnvisStep {
        match c {
            b'\\' => {
                self.cp = c;
                self.state = State::Ground as u8;
                UnvisStep::Valid(c)
            }
            b'0'..=b'7' => {
                self.cp = c.wrapping_sub(b'0');
                self.state = State::Octal2 as u8;
                UnvisStep::NoChar
            }
            b'M' => {
                self.cp = 0o200;
                self.state = State::Meta as u8;
                UnvisStep::NoChar
            }
            b'^' => {
                self.state = State::Ctrl as u8;
                UnvisStep::NoChar
            }
            b'n' => self.valid(b'\n'),
            b'r' => self.valid(b'\r'),
            b'b' => self.valid(0x08),
            b'a' => self.valid(0x07),
            b'v' => self.valid(0x0b),
            b't' => self.valid(b'\t'),
            b'f' => self.valid(0x0c),
            b's' => self.valid(b' '),
            b'E' => self.valid(0x1b),
            b'x' => {
                self.state = State::Hex as u8;
                UnvisStep::NoChar
            }
            b'\n' | b'$' => {
                self.state = State::Ground as u8;
                UnvisStep::NoChar
            }
            _ => {
                if c.is_ascii_graphic() {
                    self.cp = c;
                    self.state = State::Ground as u8;
                    UnvisStep::Valid(c)
                } else {
                    self.bad()
                }
            }
        }
    }

    fn valid(&mut self, c: u8) -> UnvisStep {
        self.cp = c;
        self.state = State::Ground as u8;
        UnvisStep::Valid(c)
    }

    fn amp(&mut self, c: u8) -> UnvisStep {
        self.cp = 0;
        if c == b'#' {
            self.state = State::Number as u8;
            return UnvisStep::NoChar;
        }
        self.nv_index = 0;
        self.state = State::String as u8;
        self.string(c)
    }

    fn string(&mut self, c: u8) -> UnvisStep {
        let first = usize::from(self.cp);
        let position = usize::from(self.nv_index);
        let last = if position == 0 {
            0
        } else {
            NV.get(first)
                .and_then(|row| row.0.get(position.wrapping_sub(1)))
                .copied()
                .unwrap_or(0)
        };
        let uc = if c == b';' { 0 } else { c };
        let mut found = None;
        let mut index = first;
        while let Some((name, value)) = NV.get(index) {
            if position != 0 && name.get(position.wrapping_sub(1)).copied().unwrap_or(0) != last {
                return self.bad();
            }
            if name.get(position).copied().unwrap_or(0) == uc {
                found = Some((index, *value));
                break;
            }
            index = index.wrapping_add(1);
        }
        let Some((index, value)) = found else {
            return self.bad();
        };
        if uc != 0 {
            self.cp = u8::try_from(index).unwrap_or(0xff);
            self.nv_index = self.nv_index.wrapping_add(1);
            self.state = State::String as u8;
            UnvisStep::NoChar
        } else {
            self.cp = value;
            self.state = State::Ground as u8;
            UnvisStep::Valid(value)
        }
    }

    fn number(&mut self, c: u8) -> UnvisStep {
        if c == b';' {
            return UnvisStep::Valid(self.cp);
        }
        if !c.is_ascii_digit() {
            return self.bad();
        }
        let digit = c.wrapping_sub(b'0');
        if u16::from(self.cp).wrapping_mul(10) > 255_u16.wrapping_sub(u16::from(digit)) {
            return self.bad();
        }
        self.cp = self.cp.wrapping_mul(10).wrapping_add(digit);
        UnvisStep::NoChar
    }

    /// Pack the machine into the C `int *astate` word.
    #[must_use]
    pub fn pack(&self) -> u32 {
        u32::from(self.state) | (u32::from(self.nv_index) << 8) | (u32::from(self.cp) << 16)
    }

    /// Unpack the C `int *astate` word.
    #[must_use]
    pub fn unpack(packed: u32) -> Self {
        Self {
            state: (packed & 0xff) as u8,
            nv_index: ((packed >> 8) & 0xff) as u8,
            cp: ((packed >> 16) & 0xff) as u8,
        }
    }

    /// Flush the state machine (`unvis(..., UNVIS_END)`).
    pub fn finish(&mut self) -> UnvisStep {
        let state = state_from(self.state);
        match state {
            State::Octal2 | State::Octal3 | State::Hex2 => {
                self.state = State::Ground as u8;
                UnvisStep::Valid(self.cp)
            }
            State::Ground => UnvisStep::NoChar,
            _ => self.bad(),
        }
    }

    fn bad(&mut self) -> UnvisStep {
        self.state = State::Ground as u8;
        UnvisStep::SynBad
    }
}

#[allow(clippy::never_loop)] // the donor's pushback path breaks out to re-feed the byte
fn decode_into(dst: &mut [u8], src: &[u8], flags: u32) -> Option<usize> {
    let mut machine = Unvis::new();
    let mut written = 0usize;
    let mut index = 0usize;
    let mut again = false;
    while index < src.len() {
        let c = src[index];
        loop {
            match machine.feed(c, flags) {
                UnvisStep::Valid(byte) => {
                    if written >= dst.len() {
                        return None;
                    }
                    dst[written] = byte;
                    written = written.wrapping_add(1);
                    break;
                }
                UnvisStep::ValidPush(byte) => {
                    if written >= dst.len() {
                        return None;
                    }
                    dst[written] = byte;
                    written = written.wrapping_add(1);
                    again = true;
                    break;
                }
                UnvisStep::NoChar => break,
                UnvisStep::SynBad | UnvisStep::Error => return None,
            }
        }
        if again {
            again = false;
            continue;
        }
        index = index.wrapping_add(1);
    }
    if let UnvisStep::Valid(byte) = machine.finish() {
        if written >= dst.len() {
            return None;
        }
        dst[written] = byte;
        written = written.wrapping_add(1);
    }
    Some(written)
}

/// `strunvis`: decode `src` into `dst`, returning the byte count.
#[must_use]
pub fn strunvis(dst: &mut [u8], src: &[u8], flags: u32) -> Option<usize> {
    decode_into(dst, src, flags)
}

/// `strnunvis`: decode `src` into `dst`, `None` when `dst` is too small.
#[must_use]
pub fn strnunvis(dst: &mut [u8], src: &[u8], flags: u32) -> Option<usize> {
    decode_into(dst, src, flags)
}

/// `strunvisx`: `strunvis` with explicit flags.
#[must_use]
pub fn strunvisx(dst: &mut [u8], src: &[u8], flags: u32) -> Option<usize> {
    decode_into(dst, src, flags)
}

/// `strnunvisx`: `strnunvis` with explicit flags.
#[must_use]
pub fn strnunvisx(dst: &mut [u8], src: &[u8], flags: u32) -> Option<usize> {
    decode_into(dst, src, flags)
}

#[cfg(test)]
#[path = "unvis_ut.rs"]
mod tests;
