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
//	@(#)regerror.c	8.4 (Berkeley) 3/20/94
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/regex/regerror.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// Altered: transpiled to Rust; hardening limits MAX_STRIP, BACKREF_BUDGET, MAX_PAREN_DEPTH, BACKREF_DEPTH_MAX; generic state-set engine; wide-character access through RegexLocale.

//! Error messages and the `REG_ATOI`/`REG_ITOA` conversions (`regerror.c`).
//!
//! The message strings are part of the ABI: the donor test driver compares them. `REG_ATOI`
//! needs the name string the shell keeps in `regex_t.re_endp`; the Rust [`crate::regerror()`]
//! signature of the contract has no such argument, so the name conversion is [`regatoi`],
//! and `regerror(REG_ATOI, ...)` reports the donor's unknown-name result (`"0"`).

use crate::Regex;
use crate::consts::{
    REG_ASSERT, REG_ATOI, REG_BADBR, REG_BADPAT, REG_BADRPT, REG_EBRACE, REG_EBRACK, REG_ECOLLATE,
    REG_ECTYPE, REG_EESCAPE, REG_EMPTY, REG_EPAREN, REG_ERANGE, REG_ESPACE, REG_ESUBREG,
    REG_ILLSEQ, REG_INVARG, REG_ITOA, REG_NOMATCH,
};

/// One row of the donor's `rerrs[]`.
struct Rerr {
    /// Numeric code.
    code: i32,
    /// `REG_*` name.
    name: &'static [u8],
    /// Human-readable explanation.
    explain: &'static [u8],
}

/// The donor's table, in donor order.
static RERRS: [Rerr; 17] = [
    Rerr {
        code: REG_NOMATCH,
        name: b"REG_NOMATCH",
        explain: b"regexec() failed to match",
    },
    Rerr {
        code: REG_BADPAT,
        name: b"REG_BADPAT",
        explain: b"invalid regular expression",
    },
    Rerr {
        code: REG_ECOLLATE,
        name: b"REG_ECOLLATE",
        explain: b"invalid collating element",
    },
    Rerr {
        code: REG_ECTYPE,
        name: b"REG_ECTYPE",
        explain: b"invalid character class",
    },
    Rerr {
        code: REG_EESCAPE,
        name: b"REG_EESCAPE",
        explain: b"trailing backslash (\\)",
    },
    Rerr {
        code: REG_ESUBREG,
        name: b"REG_ESUBREG",
        explain: b"invalid backreference number",
    },
    Rerr {
        code: REG_EBRACK,
        name: b"REG_EBRACK",
        explain: b"brackets ([ ]) not balanced",
    },
    Rerr {
        code: REG_EPAREN,
        name: b"REG_EPAREN",
        explain: b"parentheses not balanced",
    },
    Rerr {
        code: REG_EBRACE,
        name: b"REG_EBRACE",
        explain: b"braces not balanced",
    },
    Rerr {
        code: REG_BADBR,
        name: b"REG_BADBR",
        explain: b"invalid repetition count(s)",
    },
    Rerr {
        code: REG_ERANGE,
        name: b"REG_ERANGE",
        explain: b"invalid character range",
    },
    Rerr {
        code: REG_ESPACE,
        name: b"REG_ESPACE",
        explain: b"out of memory",
    },
    Rerr {
        code: REG_BADRPT,
        name: b"REG_BADRPT",
        explain: b"repetition-operator operand invalid",
    },
    Rerr {
        code: REG_EMPTY,
        name: b"REG_EMPTY",
        explain: b"empty (sub)expression",
    },
    Rerr {
        code: REG_ASSERT,
        name: b"REG_ASSERT",
        explain: b"\"can't happen\" -- you found a bug",
    },
    Rerr {
        code: REG_INVARG,
        name: b"REG_INVARG",
        explain: b"invalid argument to regex routine",
    },
    Rerr {
        code: REG_ILLSEQ,
        name: b"REG_ILLSEQ",
        explain: b"illegal byte sequence",
    },
];

/// The donor's `{0, "", "*** unknown regexp error code ***"}` sentinel.
static UNKNOWN: Rerr = Rerr {
    code: 0,
    name: b"",
    explain: b"*** unknown regexp error code ***",
};

/// Find a table row by number; the sentinel when the number is unknown.
fn find_code(code: i32) -> &'static Rerr {
    RERRS
        .iter()
        .find(|row| row.code == code)
        .unwrap_or(&UNKNOWN)
}

/// Find a table row by name; the sentinel when the name is unknown.
fn find_name(name: &[u8]) -> &'static Rerr {
    RERRS
        .iter()
        .find(|row| row.name == name)
        .unwrap_or(&UNKNOWN)
}

/// The donor's `strlcpy` into the caller's buffer.
fn copy_out(buf: &mut [u8], source: &[u8]) {
    if let Some(last) = buf.len().checked_sub(1) {
        let count = source.len().min(last);
        for (slot, &byte) in buf.iter_mut().zip(source.iter()).take(count) {
            *slot = byte;
        }
        if let Some(slot) = buf.get_mut(count) {
            *slot = 0;
        }
    }
}

/// Convert an error name such as `REG_BADPAT` to its number; `0` if unknown (`REG_ATOI`).
#[must_use]
pub fn regatoi(name: &[u8]) -> i32 {
    find_name(name).code
}

/// Render `code` in decimal into `buf`, right-aligned at its end; return the length.
fn decimal(code: i32, buf: &mut [u8; 24]) -> usize {
    let mut value = code.unsigned_abs();
    let mut at = buf.len();
    loop {
        let digit = value.wrapping_rem(10) as u8;
        value = value.wrapping_div(10);
        if let Some(slot) = at.checked_sub(1) {
            at = slot;
            if let Some(slot) = buf.get_mut(at) {
                *slot = b'0'.wrapping_add(digit);
            }
        }
        if value == 0 {
            break;
        }
    }
    if code < 0
        && let Some(slot) = at.checked_sub(1)
    {
        at = slot;
        if let Some(slot) = buf.get_mut(at) {
            *slot = b'-';
        }
    }
    let length = buf.len().saturating_sub(at);
    buf.copy_within(at.., 0);
    length
}

/// Render `REG_0x` plus the lowercase hex digits of `target` at the start of `buf`.
fn hex_name(mut target: u32, buf: &mut [u8; 32]) -> usize {
    const PREFIX: &[u8] = b"REG_0x";
    let mut digits = [0u8; 8];
    let mut count = 0usize;
    loop {
        let digit = (target & 0xf) as u8;
        target >>= 4;
        if let Some(slot) = digits.get_mut(count) {
            *slot = digit;
        }
        count = count.wrapping_add(1);
        if target == 0 {
            break;
        }
    }
    let total = PREFIX.len().saturating_add(count);
    if total > buf.len() {
        return 0;
    }
    for (index, &byte) in PREFIX.iter().enumerate() {
        if let Some(slot) = buf.get_mut(index) {
            *slot = byte;
        }
    }
    for index in 0..count {
        let digit = digits
            .get(count.saturating_sub(index).saturating_sub(1))
            .copied()
            .unwrap_or(0);
        let byte = if digit < 10 {
            b'0'.wrapping_add(digit)
        } else {
            b'a'.wrapping_add(digit).wrapping_sub(10)
        };
        if let Some(slot) = buf.get_mut(PREFIX.len().wrapping_add(index)) {
            *slot = byte;
        }
    }
    total
}

/// `regerror`: the message for `code`, written into `buf` when there is room.
///
/// Returns the length including the terminating NUL that the full message needs, exactly as
/// the donor. `buf` may be empty. `re` is accepted for contract compatibility; `REG_ATOI`'s
/// name lives in the shell's `re_endp` and is converted through [`regatoi`].
#[must_use]
pub fn regerror(code: i32, re: Option<&Regex>, buf: &mut [u8]) -> usize {
    let _ = re;
    let mut conv = [0u8; 32];
    let mut dec = [0u8; 24];
    let target = code & !REG_ITOA;
    let message: &[u8];

    if code == REG_ATOI {
        let length = decimal(0, &mut dec);
        message = dec.get(..length).unwrap_or(b"0");
    } else {
        let row = find_code(target);
        if code & REG_ITOA != 0 {
            if row.code != 0 {
                message = row.name;
            } else {
                let length = hex_name(target as u32, &mut conv);
                message = conv.get(..length).unwrap_or(b"REG_0x0");
            }
        } else {
            message = row.explain;
        }
    }

    let length = message.len().saturating_add(1);
    if !buf.is_empty() {
        copy_out(buf, message);
    }
    length
}

/// The donor's `regatoi` as a string: write the decimal code for `name` into `buf`.
///
/// Returns the number of bytes written (no NUL), `0` when `buf` is too small. This is the
/// `REG_ATOI` helper for the shell, which owns the name pointer.
#[must_use]
pub fn regatoi_str(name: &[u8], buf: &mut [u8]) -> usize {
    let mut dec = [0u8; 24];
    let code = regatoi(name);
    let length = decimal(code, &mut dec);
    if length <= buf.len() {
        for (slot, &byte) in buf.iter_mut().zip(dec.iter()).take(length) {
            *slot = byte;
        }
        length
    } else {
        0
    }
}

#[cfg(test)]
#[path = "regerror_ut.rs"]
mod tests;
