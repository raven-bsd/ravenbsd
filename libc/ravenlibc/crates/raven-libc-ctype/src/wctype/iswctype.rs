// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/ctype/iswctype.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file header upstream; covered by musl/COPYRIGHT)
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

//! `wctype` and `iswctype`; `wctype_t` is `u64` (doc 14 3.A.2).

use super::{
    iswalnum, iswalpha, iswblank, iswcntrl, iswdigit, iswgraph, iswlower, iswprint, iswpunct,
    iswspace, iswupper, iswxdigit,
};

/// `WCTYPE_ALNUM`.
pub const WCTYPE_ALNUM: u64 = 1;
/// `WCTYPE_ALPHA`.
pub const WCTYPE_ALPHA: u64 = 2;
/// `WCTYPE_BLANK`.
pub const WCTYPE_BLANK: u64 = 3;
/// `WCTYPE_CNTRL`.
pub const WCTYPE_CNTRL: u64 = 4;
/// `WCTYPE_DIGIT`.
pub const WCTYPE_DIGIT: u64 = 5;
/// `WCTYPE_GRAPH`.
pub const WCTYPE_GRAPH: u64 = 6;
/// `WCTYPE_LOWER`.
pub const WCTYPE_LOWER: u64 = 7;
/// `WCTYPE_PRINT`.
pub const WCTYPE_PRINT: u64 = 8;
/// `WCTYPE_PUNCT`.
pub const WCTYPE_PUNCT: u64 = 9;
/// `WCTYPE_SPACE`.
pub const WCTYPE_SPACE: u64 = 10;
/// `WCTYPE_UPPER`.
pub const WCTYPE_UPPER: u64 = 11;
/// `WCTYPE_XDIGIT`.
pub const WCTYPE_XDIGIT: u64 = 12;

/// Classify `wc` under a type value returned by [`wctype`].
#[must_use]
pub fn iswctype(wc: u32, t: u64) -> bool {
    match t {
        WCTYPE_ALNUM => iswalnum(wc),
        WCTYPE_ALPHA => iswalpha(wc),
        WCTYPE_BLANK => iswblank(wc),
        WCTYPE_CNTRL => iswcntrl(wc),
        WCTYPE_DIGIT => iswdigit(wc),
        WCTYPE_GRAPH => iswgraph(wc),
        WCTYPE_LOWER => iswlower(wc),
        WCTYPE_PRINT => iswprint(wc),
        WCTYPE_PUNCT => iswpunct(wc),
        WCTYPE_SPACE => iswspace(wc),
        WCTYPE_UPPER => iswupper(wc),
        WCTYPE_XDIGIT => iswxdigit(wc),
        _ => false,
    }
}

/// Map a class name (without NUL) to its `wctype_t`; unknown names return `0`.
#[must_use]
pub fn wctype(name: &[u8]) -> u64 {
    match name {
        b"alnum" => WCTYPE_ALNUM,
        b"alpha" => WCTYPE_ALPHA,
        b"blank" => WCTYPE_BLANK,
        b"cntrl" => WCTYPE_CNTRL,
        b"digit" => WCTYPE_DIGIT,
        b"graph" => WCTYPE_GRAPH,
        b"lower" => WCTYPE_LOWER,
        b"print" => WCTYPE_PRINT,
        b"punct" => WCTYPE_PUNCT,
        b"space" => WCTYPE_SPACE,
        b"upper" => WCTYPE_UPPER,
        b"xdigit" => WCTYPE_XDIGIT,
        _ => 0,
    }
}

#[cfg(test)]
#[path = "iswctype_ut.rs"]
mod tests;
