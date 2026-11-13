// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2024 The NetBSD Foundation, Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/locale/mbrtoc8.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// $NetBSD: mbrtoc8.c,v 1.8 2024/08/21 18:36:11 rillig Exp $
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
// ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
// PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
// BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.

//! `mbrtoc8`: UTF-8 to C23 `char8_t` code units.
//!
//! The NetBSD state struct does not fit the 8-byte `mbstate_t`. Re-layout: word 0 is the inner
//! `mbrtowc` state (musl encoding), word 1 holds `nleft` in the low byte and the three pending
//! continuation bytes above it. RD-new D-ct-6.

use super::{Mbr, Mbr8, mbrtowc};
use crate::mode::MbMode;
use crate::state::MbState;

fn pending_byte(word: u32, index: usize) -> u8 {
    word.wrapping_shr(8u32.wrapping_add((index as u32).wrapping_mul(8))) as u8
}

fn clear_pending_byte(word: u32, index: usize) -> u32 {
    let shift = 8u32.wrapping_add((index as u32).wrapping_mul(8));
    word & !(0xffu32.wrapping_shl(shift))
}

fn set_pending_byte(word: u32, index: usize, value: u8) -> u32 {
    clear_pending_byte(word, index)
        | u32::from(value).wrapping_shl(8u32.wrapping_add((index as u32).wrapping_mul(8)))
}

/// Decode one UTF-8 code unit, yielding trailing bytes on later calls with C return `-3`.
#[must_use]
pub fn mbrtoc8(state: &mut MbState, mode: MbMode, input: &[u8]) -> Mbr8 {
    let nleft = (state.opaque2 & 0xff) as usize;
    if nleft != 0 {
        let index = 3usize.saturating_sub(nleft);
        let unit = pending_byte(state.opaque2, index);
        let word = clear_pending_byte(state.opaque2, index);
        state.opaque2 = (word & 0xffff_ff00) | (nleft as u32).wrapping_sub(1);
        return Mbr8::Continuation { unit };
    }

    match mbrtowc(state, mode, input) {
        Mbr::Char { wc, used } => {
            if wc <= 0x7f {
                return Mbr8::Char {
                    unit: wc as u8,
                    used,
                };
            }
            if wc <= 0x7ff {
                let lead = 0xc0 | (wc >> 6) as u8;
                let mut word = set_pending_byte(0, 2, 0x80 | (wc & 0x3f) as u8);
                word |= 1;
                state.opaque2 = word;
                return Mbr8::Char { unit: lead, used };
            }
            if wc <= 0xffff {
                let lead = 0xe0 | (wc >> 12) as u8;
                let mut word = set_pending_byte(0, 1, 0x80 | ((wc >> 6) & 0x3f) as u8);
                word = set_pending_byte(word, 2, 0x80 | (wc & 0x3f) as u8);
                word |= 2;
                state.opaque2 = word;
                return Mbr8::Char { unit: lead, used };
            }
            if wc <= 0x10ffff {
                let lead = 0xf0 | (wc >> 18) as u8;
                let mut word = set_pending_byte(0, 0, 0x80 | ((wc >> 12) & 0x3f) as u8);
                word = set_pending_byte(word, 1, 0x80 | ((wc >> 6) & 0x3f) as u8);
                word = set_pending_byte(word, 2, 0x80 | (wc & 0x3f) as u8);
                word |= 3;
                state.opaque2 = word;
                return Mbr8::Char { unit: lead, used };
            }
            Mbr8::Ilseq
        }
        Mbr::Incomplete => Mbr8::Incomplete,
        Mbr::Ilseq => Mbr8::Ilseq,
    }
}

#[cfg(test)]
#[path = "mbrtoc8_ut.rs"]
mod tests;
