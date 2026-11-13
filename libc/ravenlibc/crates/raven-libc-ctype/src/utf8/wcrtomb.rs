// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/multibyte/wcrtomb.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `wcrtomb` and `wctomb`: encode one wide code unit.

use super::internal::is_codeunit;
use crate::errors::MbError;
use crate::mode::MbMode;

fn put(out: &mut [u8; 4], index: usize, value: u8) -> Result<(), MbError> {
    let Some(slot) = out.get_mut(index) else {
        return Err(MbError::Ilseq);
    };
    *slot = value;
    Ok(())
}

/// Encode `wc` into `out`; the number of bytes written is returned.
///
/// # Errors
///
/// `Ilseq` for surrogates, values above `0x10FFFF`, and (in [`MbMode::Byte`]) every value that
/// is neither ASCII nor a code unit.
pub fn wcrtomb(mode: MbMode, wc: u32, out: &mut [u8; 4]) -> Result<usize, MbError> {
    if wc < 0x80 {
        put(out, 0, wc as u8)?;
        return Ok(1);
    }
    if mode == MbMode::Byte {
        if !is_codeunit(wc) {
            return Err(MbError::Ilseq);
        }
        put(out, 0, wc as u8)?;
        return Ok(1);
    }
    if wc < 0x800 {
        put(out, 0, 0xc0 | (wc >> 6) as u8)?;
        put(out, 1, 0x80 | (wc & 0x3f) as u8)?;
        return Ok(2);
    }
    if wc < 0xd800 || wc.wrapping_sub(0xe000) < 0x2000 {
        put(out, 0, 0xe0 | (wc >> 12) as u8)?;
        put(out, 1, 0x80 | ((wc >> 6) & 0x3f) as u8)?;
        put(out, 2, 0x80 | (wc & 0x3f) as u8)?;
        return Ok(3);
    }
    if wc.wrapping_sub(0x10000) < 0x100000 {
        put(out, 0, 0xf0 | (wc >> 18) as u8)?;
        put(out, 1, 0x80 | ((wc >> 12) & 0x3f) as u8)?;
        put(out, 2, 0x80 | ((wc >> 6) & 0x3f) as u8)?;
        put(out, 3, 0x80 | (wc & 0x3f) as u8)?;
        return Ok(4);
    }
    Err(MbError::Ilseq)
}

#[cfg(test)]
#[path = "wcrtomb_ut.rs"]
mod tests;
