// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990, 1993
//	The Regents of the University of California.  All rights reserved.
//
// This code is derived from software contributed to Berkeley by
// Chris Torek.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from bionic:libc/stdio/parsefloat.c @ 731631f300090436d7f5df80d50b6275c8c60a93
// (the narrow parsefloat state machine; wparsefloat is the same machine over u32 and is not
// needed because the scan engine is generic over the input unit)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! The `scanf` floating-point syntax lexer.
//!
//! The lexer only decides the token; the value is [`crate::FloatScan`]'s job. It accepts
//! `inf[inity]`, `nan[(n-char-sequence)]`, decimal and hexadecimal forms, keeps the first
//! character that is not part of the token unread, and caps the stored token at
//! [`crate::SCAN_TOKEN_MAX`] bytes (RD-31): further matching characters are consumed but not
//! stored. The 4 KiB buffer lives in this `#[inline(never)]` function so an integer-only
//! `scanf` frame stays small.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The states mirror the donor's; indices are bounded by the caller's SCAN_TOKEN_MAX slice
// and the fixed pushback stack.

use crate::scanf::source::{Limited, ScanSource};
use crate::traits::CharUnit;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Start,
    GotSign,
    Inf,
    Nan,
    MaybeHex,
    Digits,
    Frac,
    Exp,
    ExpDigits,
}

fn is_digit(c: u32) -> bool {
    (0x30..=0x39).contains(&c)
}

fn is_xdigit(c: u32) -> bool {
    is_digit(c) || (0x61..=0x66).contains(&c) || (0x41..=0x46).contains(&c)
}

fn is_alnum(c: u32) -> bool {
    is_digit(c) || (0x61..=0x7a).contains(&c) || (0x41..=0x5a).contains(&c)
}

fn inf_match(c: u32, pos: i32) -> bool {
    const LOWER: &[u8; 7] = b"nfinity";
    const UPPER: &[u8; 7] = b"NFINITY";
    let idx = pos as usize;
    let Some(&lo) = LOWER.get(idx) else {
        return false;
    };
    let hi = UPPER.get(idx).copied().unwrap_or(lo);
    c == u32::from(lo) || c == u32::from(hi)
}

/// Lex one floating-point token into `buf`; returns the stored byte length.
///
/// The caller must pass the already width-limited cursor. Characters after the returned
/// length are pushed back into the cursor (up to its pushback depth).
#[inline(never)]
pub(crate) fn parse_token<C: CharUnit, S: ScanSource<C>>(
    f: &mut Limited<'_, S, C>,
    buf: &mut [u8],
) -> usize {
    let mut state = State::Start;
    let mut infnanpos = 0i32;
    let mut gotmantdig = false;
    let mut ishex = false;
    let mut commit_total = 0usize;
    let mut total = 0usize;
    let mut n = 0usize;
    let mut rejected: Option<C> = None;

    'read: while let Some(ch) = f.getc() {
        let c = ch.to_u32();
        if c > 0x7f {
            rejected = Some(ch);
            break;
        }
        let mut commit_now = false;
        'state: loop {
            match state {
                State::Start => {
                    state = State::GotSign;
                    if c == u32::from(b'-') || c == u32::from(b'+') {
                        break 'state;
                    }
                    continue 'state;
                }
                State::GotSign => match c as u8 {
                    b'0' => {
                        state = State::MaybeHex;
                        commit_now = true;
                    }
                    b'i' | b'I' => state = State::Inf,
                    b'n' | b'N' => state = State::Nan,
                    _ => {
                        state = State::Digits;
                        continue 'state;
                    }
                },
                State::Inf => {
                    if infnanpos > 6 || !inf_match(c, infnanpos) {
                        rejected = Some(ch);
                        break 'read;
                    }
                    if infnanpos == 1 || infnanpos == 6 {
                        commit_now = true;
                    }
                    infnanpos = infnanpos.wrapping_add(1);
                }
                State::Nan => {
                    match infnanpos {
                        -1 => {
                            rejected = Some(ch);
                            break 'read;
                        }
                        0 => {
                            if c != u32::from(b'A') && c != u32::from(b'a') {
                                rejected = Some(ch);
                                break 'read;
                            }
                        }
                        1 => {
                            if c != u32::from(b'N') && c != u32::from(b'n') {
                                rejected = Some(ch);
                                break 'read;
                            }
                            commit_now = true;
                        }
                        2 => {
                            if c != u32::from(b'(') {
                                rejected = Some(ch);
                                break 'read;
                            }
                        }
                        _ => {
                            if c == u32::from(b')') {
                                commit_now = true;
                                infnanpos = -2;
                            } else if !is_alnum(c) && c != u32::from(b'_') {
                                rejected = Some(ch);
                                break 'read;
                            }
                        }
                    }
                    infnanpos = infnanpos.wrapping_add(1);
                }
                State::MaybeHex => {
                    state = State::Digits;
                    if c == u32::from(b'X') || c == u32::from(b'x') {
                        ishex = true;
                    } else {
                        gotmantdig = true;
                        continue 'state;
                    }
                }
                State::Digits => {
                    if (ishex && is_xdigit(c)) || is_digit(c) {
                        gotmantdig = true;
                    } else {
                        state = State::Frac;
                        if c != u32::from(b'.') {
                            continue 'state;
                        }
                    }
                    if gotmantdig {
                        commit_now = true;
                    }
                }
                State::Frac => {
                    if ((c == u32::from(b'E') || c == u32::from(b'e')) && !ishex)
                        || ((c == u32::from(b'P') || c == u32::from(b'p')) && ishex)
                    {
                        if !gotmantdig {
                            rejected = Some(ch);
                            break 'read;
                        }
                        state = State::Exp;
                    } else if (ishex && is_xdigit(c)) || is_digit(c) {
                        commit_now = true;
                        gotmantdig = true;
                    } else {
                        rejected = Some(ch);
                        break 'read;
                    }
                }
                State::Exp => {
                    state = State::ExpDigits;
                    if c == u32::from(b'-') || c == u32::from(b'+') {
                        break 'state;
                    }
                    continue 'state;
                }
                State::ExpDigits => {
                    if is_digit(c) {
                        commit_now = true;
                    } else {
                        rejected = Some(ch);
                        break 'read;
                    }
                }
            }
            break 'state;
        }
        total = total.saturating_add(1);
        if n < buf.len() {
            buf[n] = c as u8;
            n = n.saturating_add(1);
        }
        if commit_now {
            commit_total = total;
        }
    }

    if let Some(ch) = rejected {
        f.ungetc(ch);
    }
    let commit = core::cmp::min(commit_total, buf.len());
    let mut idx = n;
    while idx > commit {
        idx = idx.saturating_sub(1);
        let b = buf.get(idx).copied().unwrap_or(0);
        f.ungetc(C::from_u32(u32::from(b)));
    }
    if let Some(slot) = buf.get_mut(commit) {
        *slot = 0;
    }
    commit
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "float_lex_ut.rs"]
mod tests;
