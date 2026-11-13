// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/multibyte/mbrtowc.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `mbrtowc` and its `s == NULL` reset (T-1, T-2).

use super::Mbr;
use super::bittab::entry;
use super::internal::{codeunit, oob};
use crate::errors::MbError;
use crate::mode::MbMode;
use crate::state::MbState;

/// Decode one character, keeping the partial-character state in `state.opaque1`.
///
/// A zero-length input returns [`Mbr::Incomplete`] without touching the state (T-1). In
/// [`MbMode::Byte`] a byte `>= 0x80` decodes to a code unit (`0xDF80..=0xDFFF`), never to a
/// Unicode scalar (A-ctype-2).
#[must_use]
pub fn mbrtowc(state: &mut MbState, mode: MbMode, input: &[u8]) -> Mbr {
    if input.is_empty() {
        return Mbr::Incomplete;
    }

    let mut cursor = 0usize;
    let mut c = state.opaque1;

    if c == 0 {
        let Some(&first) = input.first() else {
            return Mbr::Incomplete;
        };
        if first < 0x80 {
            state.opaque1 = 0;
            return Mbr::Char {
                wc: u32::from(first),
                used: 1,
            };
        }
        if mode == MbMode::Byte {
            state.opaque1 = 0;
            return Mbr::Char {
                wc: codeunit(first),
                used: 1,
            };
        }
        let Some(bits) = entry(first) else {
            state.opaque1 = 0;
            return Mbr::Ilseq;
        };
        c = bits;
        cursor = 1;
        if cursor >= input.len() {
            state.opaque1 = c;
            return Mbr::Incomplete;
        }
    }

    let Some(&second) = input.get(cursor) else {
        state.opaque1 = c;
        return Mbr::Incomplete;
    };
    if oob(c, second) {
        state.opaque1 = 0;
        return Mbr::Ilseq;
    }

    loop {
        let Some(&byte) = input.get(cursor) else {
            state.opaque1 = c;
            return Mbr::Incomplete;
        };
        c = c.wrapping_shl(6) | u32::from(byte).wrapping_sub(0x80);
        cursor = cursor.wrapping_add(1);
        if c & 0x8000_0000 == 0 {
            state.opaque1 = 0;
            return Mbr::Char {
                wc: c,
                used: cursor,
            };
        }
        let Some(&next) = input.get(cursor) else {
            state.opaque1 = c;
            return Mbr::Incomplete;
        };
        if next.wrapping_sub(0x80) >= 0x40 {
            state.opaque1 = 0;
            return Mbr::Ilseq;
        }
    }
}

/// The `mbrtowc(NULL, NULL, 0, ps)` reset: `Ok` when the state is initial, otherwise reset and
/// return `EILSEQ` (T-2).
///
/// # Errors
///
/// Returns [`MbError::Ilseq`] when the state was not initial.
pub fn mbrtowc_reset(state: &mut MbState) -> Result<(), MbError> {
    if state.is_initial() {
        Ok(())
    } else {
        *state = MbState::INITIAL;
        Err(MbError::Ilseq)
    }
}

#[cfg(test)]
#[path = "mbrtowc_ut.rs"]
mod tests;
