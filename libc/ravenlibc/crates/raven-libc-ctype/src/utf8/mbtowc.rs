// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/multibyte/mbtowc.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! Stateless `mbtowc`.

use super::bittab::entry;
use super::internal::{codeunit, oob};
use crate::errors::MbError;
use crate::mode::MbMode;

/// Decode one character without a state object.
///
/// Returns `(code unit, bytes consumed)`.
///
/// # Errors
///
/// `Ilseq` for an empty input, an invalid lead byte or a rejected/truncated sequence.
pub fn mbtowc(mode: MbMode, input: &[u8]) -> Result<(u32, usize), MbError> {
    let Some(&first) = input.first() else {
        return Err(MbError::Ilseq);
    };
    if first < 0x80 {
        return Ok((u32::from(first), if first == 0 { 0 } else { 1 }));
    }
    if mode == MbMode::Byte {
        return Ok((codeunit(first), 1));
    }
    let Some(bits) = entry(first) else {
        return Err(MbError::Ilseq);
    };
    let length = input.len();
    if length < 4 {
        let shift = 6usize.wrapping_mul(length).wrapping_sub(6);
        if (bits.wrapping_shl(shift as u32) & 0x8000_0000) != 0 {
            return Err(MbError::Ilseq);
        }
    }
    let Some(&second) = input.get(1) else {
        return Err(MbError::Ilseq);
    };
    if oob(bits, second) {
        return Err(MbError::Ilseq);
    }
    let mut c = bits.wrapping_shl(6) | u32::from(second).wrapping_sub(0x80);
    if c & 0x8000_0000 == 0 {
        return Ok((c, 2));
    }
    let Some(&third) = input.get(2) else {
        return Err(MbError::Ilseq);
    };
    if third.wrapping_sub(0x80) >= 0x40 {
        return Err(MbError::Ilseq);
    }
    c = c.wrapping_shl(6) | u32::from(third).wrapping_sub(0x80);
    if c & 0x8000_0000 == 0 {
        return Ok((c, 3));
    }
    let Some(&fourth) = input.get(3) else {
        return Err(MbError::Ilseq);
    };
    if fourth.wrapping_sub(0x80) >= 0x40 {
        return Err(MbError::Ilseq);
    }
    Ok((c.wrapping_shl(6) | u32::from(fourth).wrapping_sub(0x80), 4))
}

#[cfg(test)]
#[path = "mbtowc_ut.rs"]
mod tests;
