// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2024 The NetBSD Foundation, Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/locale/c8rtomb.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// $NetBSD: c8rtomb.c,v 1.10 2026/07/04 13:21:05 riastradh Exp $
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

//! `c8rtomb`: C23 `char8_t` code units to UTF-8.
//!
//! Re-layout: word 0 carries the 8-bit DFA state in bits 31..24 and the 24-bit partial scalar in
//! bits 23..0; word 1 is unused because `wcrtomb` is stateless. RD-new D-ct-6.

use super::c32rtomb;
use crate::errors::MbError;
use crate::mode::MbMode;
use crate::state::MbState;

const UTF8_ACCEPT: u8 = 0;
const UTF8_REJECT: u8 = 96;

static UTF8_CLASSTAB: [u8; 256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    10, 10, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    2, 11, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 4, 3, 3, 7, 6, 6, 6, 5, 10, 10, 10, 10, 10, 10, 10,
    10, 10, 10, 10,
];

static UTF8_STATETAB: [u8; 108] = [
    0, 96, 12, 36, 48, 84, 72, 60, 96, 96, 96, 24, 96, 0, 96, 96, 96, 96, 96, 96, 0, 0, 96, 96, 96,
    12, 96, 96, 96, 96, 96, 96, 96, 96, 96, 96, 96, 12, 96, 96, 96, 96, 96, 96, 12, 12, 96, 96, 96,
    96, 96, 96, 96, 96, 96, 96, 12, 12, 96, 96, 96, 36, 96, 96, 96, 96, 96, 96, 96, 36, 96, 96, 96,
    36, 96, 96, 96, 96, 96, 96, 36, 36, 96, 96, 96, 96, 96, 96, 96, 96, 96, 96, 36, 96, 96, 96, 96,
    96, 96, 96, 96, 96, 96, 96, 96, 96, 96, 96,
];

fn decode_step(state: u8, c8: u8, c32: &mut u32) -> u8 {
    let class = UTF8_CLASSTAB.get(c8 as usize).copied().unwrap_or(0);
    let base = if state == UTF8_ACCEPT {
        u32::from(c8) & (0xffu32 >> class)
    } else {
        (u32::from(c8) & 0x3f) | c32.wrapping_shl(6)
    };
    *c32 = base;
    UTF8_STATETAB
        .get(state.wrapping_add(class) as usize)
        .copied()
        .unwrap_or(UTF8_REJECT)
}

/// Encode one UTF-8 code unit, buffering a partial scalar across calls.
///
/// # Errors
///
/// `Ilseq` for invalid UTF-8.
pub fn c8rtomb(
    state: &mut MbState,
    mode: MbMode,
    c8: u8,
    out: &mut [u8; 4],
) -> Result<usize, MbError> {
    if c8 == 0 {
        state.opaque1 = 0;
        return c32rtomb(mode, 0, out);
    }
    let mut utf8_state = (state.opaque1 >> 24) as u8;
    let mut c32 = state.opaque1 & 0x00ff_ffff;
    utf8_state = decode_step(utf8_state, c8, &mut c32);
    if utf8_state == UTF8_REJECT {
        return Err(MbError::Ilseq);
    }
    if utf8_state != UTF8_ACCEPT {
        state.opaque1 = (u32::from(utf8_state) << 24) | (c32 & 0x00ff_ffff);
        return Ok(0);
    }
    state.opaque1 = 0;
    c32rtomb(mode, c32, out)
}

#[cfg(test)]
#[path = "c8rtomb_ut.rs"]
mod tests;
