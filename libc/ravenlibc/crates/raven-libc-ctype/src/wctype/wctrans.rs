// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/ctype/wctrans.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `wctrans` and `towctrans`; `wctrans_t` values 1 and 2.

use super::{towlower, towupper};

/// `toupper`.
pub const WCTRANS_TOUPPER: u64 = 1;
/// `tolower`.
pub const WCTRANS_TOLOWER: u64 = 2;

/// Map a transformation name (without NUL) to its value; unknown names return `0`.
#[must_use]
pub fn wctrans(name: &[u8]) -> u64 {
    match name {
        b"toupper" => WCTRANS_TOUPPER,
        b"tolower" => WCTRANS_TOLOWER,
        _ => 0,
    }
}

/// Apply a transformation value; `0` and unknown values are the identity.
#[must_use]
pub fn towctrans(wc: u32, t: u64) -> u32 {
    match t {
        WCTRANS_TOUPPER => towupper(wc),
        WCTRANS_TOLOWER => towlower(wc),
        _ => wc,
    }
}

#[cfg(test)]
#[path = "wctrans_ut.rs"]
mod tests;
