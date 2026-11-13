// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/ctype/iswcntrl.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `iswcntrl`.

/// `< 0x20`, `0x7F..=0x9F`, `U+2028..=U+2029`, `U+FFF9..=U+FFFB`.
#[must_use]
pub fn iswcntrl(wc: u32) -> bool {
    wc < 32
        || wc.wrapping_sub(0x7f) < 33
        || wc.wrapping_sub(0x2028) < 2
        || wc.wrapping_sub(0xfff9) < 3
}

#[cfg(test)]
#[path = "iswcntrl_ut.rs"]
mod tests;
