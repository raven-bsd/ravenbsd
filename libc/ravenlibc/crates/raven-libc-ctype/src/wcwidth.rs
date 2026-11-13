// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/ctype/wcwidth.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `wcwidth`, with the exact fast path and control flow of the donor (A-ctype-10).

use crate::unicode;

/// Display width of one code unit: `-1` for controls and noncharacters, `0`, `1` or `2`.
#[must_use]
pub fn wcwidth(wc: u32) -> i32 {
    if wc < 0xff {
        return if (wc.wrapping_add(1) & 0x7f) >= 0x21 {
            1
        } else if wc != 0 {
            -1
        } else {
            0
        };
    }
    if (wc & 0xfffe_ffff) < 0xfffe {
        return i32::from(unicode::width_bits(wc));
    }
    if (wc & 0xfffe) == 0xfffe {
        return -1;
    }
    if wc.wrapping_sub(0x20000) < 0x20000 {
        return 2;
    }
    if wc == 0xe0001 || wc.wrapping_sub(0xe0020) <= 0x5f || wc.wrapping_sub(0xe0100) <= 0xef {
        return 0;
    }
    1
}

#[cfg(test)]
#[path = "wcwidth_ut.rs"]
mod tests;
