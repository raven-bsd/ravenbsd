// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/multibyte/mbrtoc16.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `mbrtoc16`: UTF-8 to UTF-16 code units.
//!
//! The state word is shared with `mbrtowc`: a partial UTF-8 character has the high bit set, a
//! pending low surrogate is a positive value (A-ctype-4).

use super::{Mbr, Mbr16, mbrtowc};
use crate::mode::MbMode;
use crate::state::MbState;

/// Decode one UTF-16 code unit, buffering the low half of a surrogate pair when needed.
#[must_use]
pub fn mbrtoc16(state: &mut MbState, mode: MbMode, input: &[u8]) -> Mbr16 {
    let pending = state.opaque1;
    if (pending as i32) > 0 {
        state.opaque1 = 0;
        return Mbr16::PendingLow {
            unit: pending as u16,
        };
    }
    match mbrtowc(state, mode, input) {
        Mbr::Char { wc, used } => {
            if wc >= 0x10000 {
                state.opaque1 = (wc & 0x3ff).wrapping_add(0xdc00);
                let high = 0xd7c0u32.wrapping_add(wc >> 10);
                Mbr16::Char {
                    unit: high as u16,
                    used,
                }
            } else {
                Mbr16::Char {
                    unit: wc as u16,
                    used,
                }
            }
        }
        Mbr::Incomplete => Mbr16::Incomplete,
        Mbr::Ilseq => Mbr16::Ilseq,
    }
}

#[cfg(test)]
#[path = "mbrtoc16_ut.rs"]
mod tests;
