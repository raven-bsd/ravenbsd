// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/multibyte/c16rtomb.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `c16rtomb`: UTF-16 code units to UTF-8.

use super::wcrtomb;
use crate::errors::MbError;
use crate::mode::MbMode;
use crate::state::MbState;

/// Encode one UTF-16 code unit. A high surrogate stores state and returns `Ok(0)`.
///
/// # Errors
///
/// `Ilseq` for a lone low surrogate, a high surrogate followed by anything but a low surrogate,
/// and every value [`wcrtomb`] rejects.
pub fn c16rtomb(
    state: &mut MbState,
    mode: MbMode,
    c16: u16,
    out: &mut [u8; 4],
) -> Result<usize, MbError> {
    if state.opaque1 == 0 && c16.wrapping_sub(0xd800) < 0x400 {
        state.opaque1 = (u32::from(c16).wrapping_sub(0xd7c0)) << 10;
        return Ok(0);
    }
    let wc = if state.opaque1 != 0 {
        if u32::from(c16).wrapping_sub(0xdc00) >= 0x400 {
            state.opaque1 = 0;
            return Err(MbError::Ilseq);
        }
        let value = state
            .opaque1
            .wrapping_add(u32::from(c16))
            .wrapping_sub(0xdc00);
        state.opaque1 = 0;
        value
    } else {
        u32::from(c16)
    };
    wcrtomb(mode, wc, out)
}

#[cfg(test)]
#[path = "c16rtomb_ut.rs"]
mod tests;
