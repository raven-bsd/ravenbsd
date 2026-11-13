// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/multibyte/mbsnrtowcs.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `mbsnrtowcs`: bounded-length `mbsrtowcs`.
//!
//! The donor batches through `mbsrtowcs` to bound reads; this port decodes with a bounded slice
//! and produces the same observable results (return value, source position, destination prefix
//! and state). RD-new D-ct-5.

use super::{Mbr, SrcPos, StrConv, mbrtowc};
use crate::errors::MbError;
use crate::mode::MbMode;
use crate::state::MbState;

fn write(dst: &mut Option<&mut [u32]>, index: usize, value: u32) -> bool {
    let Some(slice) = dst.as_deref_mut() else {
        return false;
    };
    let Some(slot) = slice.get_mut(index) else {
        return false;
    };
    *slot = value;
    true
}

/// Convert the bounded source slice to wide code units.
#[must_use]
pub fn mbsnrtowcs(
    state: Option<&mut MbState>,
    mode: MbMode,
    src: &[u8],
    dst: Option<&mut [u32]>,
) -> StrConv {
    let mut st = state.as_ref().map_or(MbState::INITIAL, |state| **state);
    let limit = src.len();
    let mut dst = dst;
    let capacity = dst.as_ref().map_or(usize::MAX, |slice| slice.len());
    let mut offset = 0usize;
    let mut count = 0usize;

    while offset < limit {
        if dst.is_some() && count >= capacity {
            break;
        }
        match mbrtowc(&mut st, mode, src.get(offset..limit).unwrap_or(&[])) {
            Mbr::Char { wc, used } => {
                if wc == 0 {
                    if dst.is_some() {
                        write(&mut dst, count, 0);
                    }
                    if let Some(slot) = state {
                        *slot = MbState::INITIAL;
                    }
                    return StrConv {
                        ret: Ok(count),
                        src: if dst.is_none() {
                            SrcPos::At(0)
                        } else {
                            SrcPos::Nul
                        },
                        state: MbState::INITIAL,
                    };
                }
                if dst.is_some() && !write(&mut dst, count, wc) {
                    break;
                }
                count = count.wrapping_add(1);
                offset = offset.wrapping_add(used);
            }
            Mbr::Incomplete => {
                offset = limit;
                break;
            }
            Mbr::Ilseq => {
                if let Some(slot) = state {
                    *slot = MbState::INITIAL;
                }
                return StrConv {
                    ret: Err(MbError::Ilseq),
                    src: if dst.is_none() {
                        SrcPos::At(0)
                    } else {
                        SrcPos::At(offset)
                    },
                    state: MbState::INITIAL,
                };
            }
        }
    }

    if let Some(slot) = state {
        *slot = st;
    }
    StrConv {
        ret: Ok(count),
        src: if dst.is_none() {
            SrcPos::At(0)
        } else {
            SrcPos::At(offset)
        },
        state: st,
    }
}

#[cfg(test)]
#[path = "mbsnrtowcs_ut.rs"]
mod tests;
