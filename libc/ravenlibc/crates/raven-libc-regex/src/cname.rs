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
//	@(#)cname.h	8.3 (Berkeley) 3/20/94
// $FreeBSD: head/lib/libc/regex/cname.h 326025 2017-11-20 19:49:47Z pfg $
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/regex/cname.h @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// Altered: transpiled to Rust; hardening limits MAX_STRIP, BACKREF_BUDGET, MAX_PAREN_DEPTH, BACKREF_DEPTH_MAX; generic state-set engine; wide-character access through RegexLocale.

//! The `[.name.]` collating-element table (`cname.h`).
//!
//! The 95 entries were converted mechanically from the donor table; the order, the duplicate
//! spellings (`BEL`/`alert`, `hyphen`/`hyphen-minus`, ...) and the code values are unchanged.

/// One collating-element name and its byte value.
#[derive(Clone, Copy)]
pub struct Cname {
    /// The bracket-expression name, without the surrounding `[.` and `.]`.
    pub name: &'static [u8],
    /// The character the name denotes.
    pub code: u8,
}

/// The donor's `cnames[]`, in donor order.
pub const CNAMES: [Cname; 95] = [
    Cname {
        name: b"NUL",
        code: 0x00,
    },
    Cname {
        name: b"SOH",
        code: 0x01,
    },
    Cname {
        name: b"STX",
        code: 0x02,
    },
    Cname {
        name: b"ETX",
        code: 0x03,
    },
    Cname {
        name: b"EOT",
        code: 0x04,
    },
    Cname {
        name: b"ENQ",
        code: 0x05,
    },
    Cname {
        name: b"ACK",
        code: 0x06,
    },
    Cname {
        name: b"BEL",
        code: 0x07,
    },
    Cname {
        name: b"alert",
        code: 0x07,
    },
    Cname {
        name: b"BS",
        code: 0x08,
    },
    Cname {
        name: b"backspace",
        code: 0x08,
    },
    Cname {
        name: b"HT",
        code: 0x09,
    },
    Cname {
        name: b"tab",
        code: 0x09,
    },
    Cname {
        name: b"LF",
        code: 0x0a,
    },
    Cname {
        name: b"newline",
        code: 0x0a,
    },
    Cname {
        name: b"VT",
        code: 0x0b,
    },
    Cname {
        name: b"vertical-tab",
        code: 0x0b,
    },
    Cname {
        name: b"FF",
        code: 0x0c,
    },
    Cname {
        name: b"form-feed",
        code: 0x0c,
    },
    Cname {
        name: b"CR",
        code: 0x0d,
    },
    Cname {
        name: b"carriage-return",
        code: 0x0d,
    },
    Cname {
        name: b"SO",
        code: 0x0e,
    },
    Cname {
        name: b"SI",
        code: 0x0f,
    },
    Cname {
        name: b"DLE",
        code: 0x10,
    },
    Cname {
        name: b"DC1",
        code: 0x11,
    },
    Cname {
        name: b"DC2",
        code: 0x12,
    },
    Cname {
        name: b"DC3",
        code: 0x13,
    },
    Cname {
        name: b"DC4",
        code: 0x14,
    },
    Cname {
        name: b"NAK",
        code: 0x15,
    },
    Cname {
        name: b"SYN",
        code: 0x16,
    },
    Cname {
        name: b"ETB",
        code: 0x17,
    },
    Cname {
        name: b"CAN",
        code: 0x18,
    },
    Cname {
        name: b"EM",
        code: 0x19,
    },
    Cname {
        name: b"SUB",
        code: 0x1a,
    },
    Cname {
        name: b"ESC",
        code: 0x1b,
    },
    Cname {
        name: b"IS4",
        code: 0x1c,
    },
    Cname {
        name: b"FS",
        code: 0x1c,
    },
    Cname {
        name: b"IS3",
        code: 0x1d,
    },
    Cname {
        name: b"GS",
        code: 0x1d,
    },
    Cname {
        name: b"IS2",
        code: 0x1e,
    },
    Cname {
        name: b"RS",
        code: 0x1e,
    },
    Cname {
        name: b"IS1",
        code: 0x1f,
    },
    Cname {
        name: b"US",
        code: 0x1f,
    },
    Cname {
        name: b"space",
        code: 0x20,
    },
    Cname {
        name: b"exclamation-mark",
        code: 0x21,
    },
    Cname {
        name: b"quotation-mark",
        code: 0x22,
    },
    Cname {
        name: b"number-sign",
        code: 0x23,
    },
    Cname {
        name: b"dollar-sign",
        code: 0x24,
    },
    Cname {
        name: b"percent-sign",
        code: 0x25,
    },
    Cname {
        name: b"ampersand",
        code: 0x26,
    },
    Cname {
        name: b"apostrophe",
        code: 0x27,
    },
    Cname {
        name: b"left-parenthesis",
        code: 0x28,
    },
    Cname {
        name: b"right-parenthesis",
        code: 0x29,
    },
    Cname {
        name: b"asterisk",
        code: 0x2a,
    },
    Cname {
        name: b"plus-sign",
        code: 0x2b,
    },
    Cname {
        name: b"comma",
        code: 0x2c,
    },
    Cname {
        name: b"hyphen",
        code: 0x2d,
    },
    Cname {
        name: b"hyphen-minus",
        code: 0x2d,
    },
    Cname {
        name: b"period",
        code: 0x2e,
    },
    Cname {
        name: b"full-stop",
        code: 0x2e,
    },
    Cname {
        name: b"slash",
        code: 0x2f,
    },
    Cname {
        name: b"solidus",
        code: 0x2f,
    },
    Cname {
        name: b"zero",
        code: 0x30,
    },
    Cname {
        name: b"one",
        code: 0x31,
    },
    Cname {
        name: b"two",
        code: 0x32,
    },
    Cname {
        name: b"three",
        code: 0x33,
    },
    Cname {
        name: b"four",
        code: 0x34,
    },
    Cname {
        name: b"five",
        code: 0x35,
    },
    Cname {
        name: b"six",
        code: 0x36,
    },
    Cname {
        name: b"seven",
        code: 0x37,
    },
    Cname {
        name: b"eight",
        code: 0x38,
    },
    Cname {
        name: b"nine",
        code: 0x39,
    },
    Cname {
        name: b"colon",
        code: 0x3a,
    },
    Cname {
        name: b"semicolon",
        code: 0x3b,
    },
    Cname {
        name: b"less-than-sign",
        code: 0x3c,
    },
    Cname {
        name: b"equals-sign",
        code: 0x3d,
    },
    Cname {
        name: b"greater-than-sign",
        code: 0x3e,
    },
    Cname {
        name: b"question-mark",
        code: 0x3f,
    },
    Cname {
        name: b"commercial-at",
        code: 0x40,
    },
    Cname {
        name: b"left-square-bracket",
        code: 0x5b,
    },
    Cname {
        name: b"backslash",
        code: 0x5c,
    },
    Cname {
        name: b"reverse-solidus",
        code: 0x5c,
    },
    Cname {
        name: b"right-square-bracket",
        code: 0x5d,
    },
    Cname {
        name: b"circumflex",
        code: 0x5e,
    },
    Cname {
        name: b"circumflex-accent",
        code: 0x5e,
    },
    Cname {
        name: b"underscore",
        code: 0x5f,
    },
    Cname {
        name: b"low-line",
        code: 0x5f,
    },
    Cname {
        name: b"grave-accent",
        code: 0x60,
    },
    Cname {
        name: b"left-brace",
        code: 0x7b,
    },
    Cname {
        name: b"left-curly-bracket",
        code: 0x7b,
    },
    Cname {
        name: b"vertical-line",
        code: 0x7c,
    },
    Cname {
        name: b"right-brace",
        code: 0x7d,
    },
    Cname {
        name: b"right-curly-bracket",
        code: 0x7d,
    },
    Cname {
        name: b"tilde",
        code: 0x7e,
    },
    Cname {
        name: b"DEL",
        code: 0x7f,
    },
];

/// Look up a collating-element name (the donor compares the exact length too).
#[must_use]
pub fn lookup(name: &[u8]) -> Option<u8> {
    CNAMES
        .iter()
        .find(|entry| entry.name == name)
        .map(|entry| entry.code)
}

#[cfg(test)]
#[path = "cname_ut.rs"]
mod tests;
