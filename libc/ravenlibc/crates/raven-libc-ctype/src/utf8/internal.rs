// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/multibyte/internal.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! The state table macros of musl's `internal.h` as `const fn`s.
//!
//! The upper six state bits are a negative offset with which the next byte is range-checked;
//! `oob` is equivalent to `((b - 0x80) | (b + offset)) & ~0x3f`.

/// First lead byte of a well-formed UTF-8 sequence.
pub const SA: u32 = 0xc2;
/// Last lead byte of a well-formed UTF-8 sequence.
pub const SB: u32 = 0xf4;

/// `R(a, b)` of `internal.c`.
pub(crate) const fn r(a: u32, b: u32) -> u32 {
    let base = if a == 0x80 {
        0x40u32.wrapping_sub(b)
    } else {
        0u32.wrapping_sub(a)
    };
    base.wrapping_shl(23)
}

/// Whether the next byte is outside the range the partial state allows.
#[must_use]
pub const fn oob(state: u32, b: u8) -> bool {
    let top = (b as i32) >> 3;
    let low = top.wrapping_sub(0x10);
    let high = top.wrapping_add((state as i32) >> 26);
    ((low | high) & !7) != 0
}

/// `CODEUNIT(c)` of `internal.h`: a byte represented as a code unit, `0xDF80..=0xDFFF`.
#[must_use]
pub const fn codeunit(b: u8) -> u32 {
    0xdfff & ((b as i8) as i32 as u32)
}

/// `IS_CODEUNIT(c)` of `internal.h`.
#[must_use]
pub const fn is_codeunit(c: u32) -> bool {
    c.wrapping_sub(0xdf80) < 0x80
}

#[cfg(test)]
#[path = "internal_ut.rs"]
mod tests;

pub(crate) use r as range_bits;
