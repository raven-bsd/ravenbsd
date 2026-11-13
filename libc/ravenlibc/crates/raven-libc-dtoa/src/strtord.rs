// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998, 2000 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/strtord.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: strtord.c,v 1.5 2013/04/18 21:54:11 joerg Exp $)
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
//! `strtord.c` of NetBSD gdtoa: the explicit-rounding double entry and `ULtod`.
//!
//! Transpiled from `netbsd:lib/libc/gdtoa/strtord.c`. `ULtod` packs the generic parser's words
//! into a `double`; it is also the hex-float result path of `strtod.rs`.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::config::{D_QNAN0, D_QNAN1, from_words};
use crate::ctx::DtoaCtx;
use crate::types::*;
use crate::{DtoaError, NumericLocale, Rounding};

/// `ULtod(L, bits, expt, k)`: build the double bits from the parser result.
pub(crate) fn ultod(bits: &[u32], expt: i32, k: i32) -> f64 {
    let mut w0;
    let w1;
    match k & STRTOG_RETMASK {
        STRTOG_NONUMBER | STRTOG_ZERO => {
            w0 = 0;
            w1 = 0;
        }
        STRTOG_DENORMAL => {
            w1 = bits.first().copied().unwrap_or(0);
            w0 = bits.get(1).copied().unwrap_or(0);
        }
        STRTOG_NORMAL | STRTOG_NANBITS => {
            w1 = bits.first().copied().unwrap_or(0);
            w0 = (bits.get(1).copied().unwrap_or(0) & !0x0010_0000)
                | (expt.wrapping_add(0x3ff).wrapping_add(52) as u32) << 20;
        }
        STRTOG_INFINITE => {
            w0 = 0x7ff0_0000;
            w1 = 0;
        }
        _ => {
            w1 = D_QNAN0;
            w0 = D_QNAN1;
        }
    }
    if k & STRTOG_NEG != 0 {
        w0 |= 0x8000_0000;
    }
    from_words(w0, w1)
}

/// `strtord(s, sp, rounding, d)`: correctly rounded double under an explicit rounding direction.
#[allow(dead_code)] // donor entry kept for the explicit-rounding tests (strtod has its own path)
pub(crate) fn strtord(
    ctx: &mut DtoaCtx,
    s: &[u8],
    loc: &NumericLocale,
    rounding: Rounding,
) -> Result<(f64, usize, i32), DtoaError> {
    let fpi0 = Fpi::double();
    let mut bits = [0u32; 2];
    let mut expt = 0i32;
    let fpi = fpi0.with_rounding(rounding.as_fpi());
    let (consumed, k) = crate::strtodg::strtodg(ctx, s, &fpi, &mut expt, &mut bits, loc)?;
    Ok((ultod(&bits, expt, k), consumed, k))
}

#[cfg(test)]
#[path = "strtord_ut.rs"]
mod tests;
