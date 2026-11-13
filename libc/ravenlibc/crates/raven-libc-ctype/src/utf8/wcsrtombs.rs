// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/multibyte/wcsrtombs.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `wcsrtombs`: the stateful wide-to-string conversion.
//!
//! A partial character is never written: non-ASCII characters go through a 4-byte temporary
//! (T-5). With `dst == None` the length is counted and the source position is not advanced.

use super::{SrcPos, StrConv, wcrtomb};
use crate::mode::MbMode;
use crate::state::MbState;

fn write(dst: &mut Option<&mut [u8]>, index: usize, value: u8) -> bool {
    let Some(slice) = dst.as_deref_mut() else {
        return false;
    };
    let Some(slot) = slice.get_mut(index) else {
        return false;
    };
    *slot = value;
    true
}

/// Encode a wide string into a multibyte string.
#[must_use]
pub fn wcsrtombs(
    state: Option<&mut MbState>,
    mode: MbMode,
    src: &[u32],
    dst: Option<&mut [u8]>,
) -> StrConv {
    let st = state.as_ref().map_or(MbState::INITIAL, |state| **state);
    let mut dst = dst;

    if dst.is_none() {
        let mut total = 0usize;
        for &wc in src {
            if wc == 0 {
                break;
            }
            if wc < 0x80 {
                total = total.wrapping_add(1);
            } else {
                let mut temp = [0u8; 4];
                match wcrtomb(mode, wc, &mut temp) {
                    Ok(length) => total = total.wrapping_add(length),
                    Err(error) => {
                        return StrConv {
                            ret: Err(error),
                            src: SrcPos::At(0),
                            state: st,
                        };
                    }
                }
            }
        }
        return StrConv {
            ret: Ok(total),
            src: SrcPos::At(0),
            state: st,
        };
    }

    let capacity = dst.as_ref().map_or(0, |slice| slice.len());
    let mut offset = 0usize;
    let mut written = 0usize;
    while written < capacity {
        let Some(&wc) = src.get(offset) else {
            break;
        };
        if wc == 0 {
            write(&mut dst, written, 0);
            if let Some(slot) = state {
                *slot = st;
            }
            return StrConv {
                ret: Ok(written),
                src: SrcPos::Nul,
                state: st,
            };
        }
        if wc < 0x80 {
            write(&mut dst, written, wc as u8);
            written = written.wrapping_add(1);
            offset = offset.wrapping_add(1);
            continue;
        }
        let mut temp = [0u8; 4];
        let length = match wcrtomb(mode, wc, &mut temp) {
            Ok(length) => length,
            Err(error) => {
                return StrConv {
                    ret: Err(error),
                    src: SrcPos::At(offset),
                    state: st,
                };
            }
        };
        if length > capacity.saturating_sub(written) {
            break;
        }
        for &byte in temp.get(..length).unwrap_or(&[]) {
            write(&mut dst, written, byte);
            written = written.wrapping_add(1);
        }
        offset = offset.wrapping_add(1);
    }

    if let Some(slot) = state {
        *slot = st;
    }
    StrConv {
        ret: Ok(written),
        src: SrcPos::At(offset),
        state: st,
    }
}

#[cfg(test)]
#[path = "wcsrtombs_ut.rs"]
mod tests;
