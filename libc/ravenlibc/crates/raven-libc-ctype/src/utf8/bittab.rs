// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/multibyte/internal.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! The 51-entry UTF-8 partial-character table, one entry for each lead byte `0xC2..=0xF4`.

use super::internal::{SA, range_bits};

/// `C(x)` of `internal.c`.
const fn c(x: u32) -> u32 {
    if x < 2 {
        u32::MAX
    } else {
        range_bits(0x80, 0xc0) | x
    }
}

/// `D(x)` of `internal.c`.
const fn d(x: u32) -> u32 {
    c(x.wrapping_add(16))
}

const fn r80c0() -> u32 {
    range_bits(0x80, 0xc0)
}

/// `E(x)` of `internal.c`.
const fn e(x: u32) -> u32 {
    let head = if x == 0 {
        range_bits(0xa0, 0xc0)
    } else if x == 0xd {
        range_bits(0x80, 0xa0)
    } else {
        r80c0()
    };
    head | (r80c0() >> 6) | x
}

/// `F(x)` of `internal.c`.
const fn f(x: u32) -> u32 {
    let head = if x >= 5 {
        0
    } else if x == 0 {
        range_bits(0x90, 0xc0)
    } else if x == 4 {
        range_bits(0x80, 0x90)
    } else {
        r80c0()
    };
    head | (r80c0() >> 6) | (r80c0() >> 12) | x
}

/// State entry for lead byte `SA + index`.
pub static BITTAB: [u32; 51] = [
    c(2),
    c(3),
    c(4),
    c(5),
    c(6),
    c(7),
    c(8),
    c(9),
    c(0xa),
    c(0xb),
    c(0xc),
    c(0xd),
    c(0xe),
    c(0xf),
    d(0x0),
    d(0x1),
    d(0x2),
    d(0x3),
    d(0x4),
    d(0x5),
    d(0x6),
    d(0x7),
    d(0x8),
    d(0x9),
    d(0xa),
    d(0xb),
    d(0xc),
    d(0xd),
    d(0xe),
    d(0xf),
    e(0x0),
    e(0x1),
    e(0x2),
    e(0x3),
    e(0x4),
    e(0x5),
    e(0x6),
    e(0x7),
    e(0x8),
    e(0x9),
    e(0xa),
    e(0xb),
    e(0xc),
    e(0xd),
    e(0xe),
    e(0xf),
    f(0x0),
    f(0x1),
    f(0x2),
    f(0x3),
    f(0x4),
];

/// The state entry for a lead byte, or `None` outside `SA..=SB`.
#[must_use]
pub fn entry(lead: u8) -> Option<u32> {
    let index = (lead as u32).checked_sub(SA)?;
    if index >= BITTAB.len() as u32 {
        return None;
    }
    BITTAB.get(index as usize).copied()
}

const _: () = assert!(BITTAB.len() == 51);

/// A byte that continues a sequence, `0x80..=0xBF`.
#[must_use]
pub const fn is_continuation(b: u8) -> bool {
    b.wrapping_sub(0x80) < 0x40
}

/// Whether `c` is a code unit and therefore not a Unicode scalar value.
#[must_use]
pub const fn code_unit(c: u32) -> bool {
    c >= 0xdf80 && c <= 0xdfff
}

#[cfg(test)]
#[path = "bittab_ut.rs"]
mod tests;
