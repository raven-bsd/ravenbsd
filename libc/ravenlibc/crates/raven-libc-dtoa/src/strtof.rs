// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998, 2000 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/strtof.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: strtof.c,v 1.7 2013/05/17 12:55:57 joerg Exp $)
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
//! `strtof.c` of NetBSD gdtoa: correctly rounded binary32, never via `double` (T-A1).
//!
//! Transpiled from `netbsd:lib/libc/gdtoa/strtof.c`; `gdtoa_fltrnds.h` becomes the [`Rounding`]
//! parameter, not a `fegetround` read.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::config::F_QNAN;
use crate::ctx::DtoaCtx;
use crate::types::*;
use crate::{DtoaError, NumericLocale, Rounding};

/// `strtof`: parse through `strtodg` with the binary32 `FPI` (nbits 24).
pub(crate) fn strtof(
    ctx: &mut DtoaCtx,
    s: &[u8],
    loc: &NumericLocale,
    rounding: Rounding,
) -> Result<(f32, usize, i32), DtoaError> {
    let fpi = Fpi::float().with_rounding(rounding.as_fpi());
    let mut bits = [0u32; 1];
    let mut expt = 0i32;
    let (consumed, k) = crate::strtodg::strtodg(ctx, s, &fpi, &mut expt, &mut bits, loc)?;
    let mut u = match k & STRTOG_RETMASK {
        STRTOG_NONUMBER | STRTOG_ZERO => 0,
        STRTOG_NORMAL | STRTOG_NANBITS => {
            (bits[0] & 0x007f_ffff) | (expt.wrapping_add(0x7f).wrapping_add(23) as u32) << 23
        }
        STRTOG_DENORMAL => bits[0],
        STRTOG_INFINITE => 0x7f80_0000,
        STRTOG_NAN => F_QNAN,
        _ => 0,
    };
    if k & STRTOG_NEG != 0 {
        u |= 0x8000_0000;
    }
    Ok((f32::from_bits(u), consumed, k))
}

#[cfg(test)]
#[path = "strtof_ut.rs"]
mod tests;
