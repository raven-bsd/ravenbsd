// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998, 2000 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/strtorx.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/strtopx.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tags: $NetBSD: strtorx.c,v 1.4 2011/03/20 23:15:35 christos Exp $;
//  $NetBSD: strtopx.c,v 1.6 2013/04/18 21:54:11 joerg Exp $)
//
// The author of this software is David M. Gay.
//
// Permission to use, copy, modify, and distribute this software and
// its documentation for any purpose and without fee is hereby
// granted, provided that the above copyright notice appear in all
// copies and that both that the copyright notice and this
// permission notice and warranty disclaimer appear in supporting
// documentation, and that the name of Lucent or any of its entities
// not be used in advertising or publicity pertaining to
// distribution of the software without specific, written prior
// permission.
//
// LUCENT DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE,
// INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS.
// IN NO EVENT SHALL LUCENT OR ANY OF ITS ENTITIES BE LIABLE FOR ANY
// SPECIAL, INDIRECT OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER
// IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
// ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
// THIS SOFTWARE.
//
// Please send bug reports to David M. Gay (dmg at acm dot org,
// with " at " changed at "@" and " dot " changed to ".").
//! The x87 80-bit packing of `strtorx.c` and `strtopx.c` (x86_64 only).
//!
//! `ULtox` builds the 16-byte slot: bytes 0..10 are the little-endian `u16[5]` image (significand
//! in words 0..4, sign+exponent in word 4), bytes 10..16 are zero.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::config::LDUS_QNAN;
use crate::ctx::DtoaCtx;
use crate::types::*;
use crate::{DtoaError, LongDouble, NumericLocale, Rounding};

/// `ULtox`: pack the parser's words into the x87 slot.
pub(crate) fn ultox(bits: &[u32], expt: i32, k: i32) -> LongDouble {
    let mut l = [0u16; 5];
    match k & STRTOG_RETMASK {
        STRTOG_NONUMBER | STRTOG_ZERO => {}
        STRTOG_DENORMAL => {
            l[4] = 0;
            l[0] = bits.first().copied().unwrap_or(0) as u16;
            l[1] = (bits.first().copied().unwrap_or(0) >> 16) as u16;
            l[2] = bits.get(1).copied().unwrap_or(0) as u16;
            l[3] = (bits.get(1).copied().unwrap_or(0) >> 16) as u16;
        }
        STRTOG_NORMAL | STRTOG_NANBITS => {
            l[4] = expt.wrapping_add(0x3fff).wrapping_add(63) as u16;
            l[0] = bits.first().copied().unwrap_or(0) as u16;
            l[1] = (bits.first().copied().unwrap_or(0) >> 16) as u16;
            l[2] = bits.get(1).copied().unwrap_or(0) as u16;
            l[3] = (bits.get(1).copied().unwrap_or(0) >> 16) as u16;
        }
        STRTOG_INFINITE => {
            l[4] = 0x7fff;
            l[3] = 0x8000;
        }
        STRTOG_NAN => {
            l = LDUS_QNAN;
        }
        _ => {}
    }
    if k & STRTOG_NEG != 0 {
        l[4] |= 0x8000;
    }
    let mut out = [0u8; 16];
    let mut i = 0usize;
    while i < 5 {
        out[i * 2] = (l[i] & 0xff) as u8;
        out[i * 2 + 1] = (l[i] >> 8) as u8;
        i += 1;
    }
    LongDouble(out)
}

/// `strtorx`: parse with an explicit rounding direction.
pub(crate) fn strtorx(
    ctx: &mut DtoaCtx,
    s: &[u8],
    loc: &NumericLocale,
    rounding: Rounding,
) -> Result<(LongDouble, usize, i32), DtoaError> {
    let fpi = Fpi::ld80().with_rounding(rounding.as_fpi());
    let mut bits = [0u32; 2];
    let mut expt = 0i32;
    let (consumed, k) = crate::strtodg::strtodg(ctx, s, &fpi, &mut expt, &mut bits, loc)?;
    Ok((ultox(&bits, expt, k), consumed, k))
}

/// `strtopx`: the entry `strtold` calls (current rounding is the [`Rounding`] parameter).
pub(crate) fn strtopx(
    ctx: &mut DtoaCtx,
    s: &[u8],
    loc: &NumericLocale,
    rounding: Rounding,
) -> Result<(LongDouble, usize, i32), DtoaError> {
    strtorx(ctx, s, loc, rounding)
}

#[cfg(test)]
#[path = "fmt_ld80_ut.rs"]
mod tests;
