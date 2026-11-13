// SPDX-License-Identifier: MIT
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/multibyte/mbsrtowcs.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `mbsrtowcs`: the stateful string-to-wide conversion.

use super::internal::codeunit;
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

fn capacity(dst: &Option<&mut [u32]>) -> usize {
    dst.as_ref().map_or(0, |slice| slice.len())
}

/// The four ASCII bytes at `offset` when they are all in `0x01..=0x7F`, else `None`.
fn ascii_word(src: &[u8], offset: usize) -> Option<[u8; 4]> {
    let chunk = src.get(offset..offset.checked_add(4)?)?;
    if let [a, b, c, d] = chunk
        && *a != 0
        && *b != 0
        && *c != 0
        && *d != 0
        && (*a | *b | *c | *d) < 0x80
    {
        return Some([*a, *b, *c, *d]);
    }
    None
}

/// Convert a multibyte string to wide code units.
///
/// `src` includes its terminating NUL. With `dst == None` the length is counted and the source
/// position is not reported as advanced (the C leaves `*src` alone). On [`MbError::Ilseq`] the
/// source position is the lead byte of the bad sequence (T-4) and the converted prefix stays in
/// `dst`.
#[must_use]
pub fn mbsrtowcs(
    state: Option<&mut MbState>,
    mode: MbMode,
    src: &[u8],
    dst: Option<&mut [u32]>,
) -> StrConv {
    let mut st = state.as_ref().map_or(MbState::INITIAL, |state| **state);
    let mut dst = dst;
    let mut count = 0usize;
    let mut offset = 0usize;

    if mode == MbMode::Byte && st.is_initial() {
        if dst.is_none() {
            let length = src.iter().position(|&byte| byte == 0).unwrap_or(src.len());
            if let Some(slot) = state {
                *slot = st;
            }
            return StrConv {
                ret: Ok(length),
                src: SrcPos::At(0),
                state: st,
            };
        }
        let limit = capacity(&dst);
        while count < limit {
            let Some(&byte) = src.get(offset) else {
                break;
            };
            if byte == 0 {
                write(&mut dst, count, 0);
                if let Some(slot) = state {
                    *slot = st;
                }
                return StrConv {
                    ret: Ok(count),
                    src: SrcPos::Nul,
                    state: st,
                };
            }
            let unit = codeunit(byte);
            if !write(&mut dst, count, unit) {
                break;
            }
            count = count.wrapping_add(1);
            offset = offset.wrapping_add(1);
        }
        if let Some(slot) = state {
            *slot = st;
        }
        return StrConv {
            ret: Ok(count),
            src: SrcPos::At(offset),
            state: st,
        };
    }

    loop {
        if dst.is_some() && count >= capacity(&dst) {
            if let Some(slot) = state {
                *slot = st;
            }
            return StrConv {
                ret: Ok(count),
                src: SrcPos::At(offset),
                state: st,
            };
        }
        if offset.is_multiple_of(4)
            && (dst.is_none() || count.saturating_add(4) < capacity(&dst))
            && let Some(word) = ascii_word(src, offset)
        {
            // Word-at-a-time ASCII fast path: the group has no NUL and no high bit.
            for byte in word {
                if dst.is_some() {
                    let unit = u32::from(byte);
                    if !write(&mut dst, count, unit) {
                        break;
                    }
                    count = count.wrapping_add(1);
                } else {
                    count = count.wrapping_add(1);
                }
            }
            offset = offset.wrapping_add(4);
            continue;
        }
        let Some(&first) = src.get(offset) else {
            if let Some(slot) = state {
                *slot = st;
            }
            return StrConv {
                ret: Err(MbError::Ilseq),
                src: SrcPos::At(offset),
                state: MbState::INITIAL,
            };
        };
        if first == 0 {
            write(&mut dst, count, 0);
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
        match mbrtowc(&mut st, mode, src.get(offset..).unwrap_or(&[])) {
            Mbr::Char { wc, used } => {
                if wc == 0 {
                    write(&mut dst, count, 0);
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
                if dst.is_some() {
                    if !write(&mut dst, count, wc) {
                        if let Some(slot) = state {
                            *slot = st;
                        }
                        return StrConv {
                            ret: Ok(count),
                            src: SrcPos::At(offset),
                            state: st,
                        };
                    }
                    count = count.wrapping_add(1);
                } else {
                    count = count.wrapping_add(1);
                }
                offset = offset.wrapping_add(used);
            }
            Mbr::Incomplete | Mbr::Ilseq => {
                if let Some(slot) = state {
                    *slot = MbState::INITIAL;
                }
                return StrConv {
                    ret: Err(MbError::Ilseq),
                    src: SrcPos::At(offset),
                    state: MbState::INITIAL,
                };
            }
        }
    }
}

#[cfg(test)]
#[path = "mbsrtowcs_ut.rs"]
mod tests;
