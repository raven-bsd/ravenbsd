// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998, 2000 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/strtorQ.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/strtopQ.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tags: $NetBSD: strtorQ.c,v 1.5 2019/08/01 02:27:43 riastradh Exp $;
//  $NetBSD: strtopQ.c,v 1.6 2013/04/18 21:54:11 joerg Exp $)
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
//! The IEEE binary128 packing of `strtorQ.c` and `strtopQ.c`.
//!
//! Built for aarch64 and riscv64, and also on x86_64 under `cfg(test)` so the ld128 property tests
//! run on the host (the public `strtold` is the platform's format).

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::config::LD_QNAN;
use crate::ctx::DtoaCtx;
use crate::types::*;
use crate::{DtoaError, LongDouble, NumericLocale, Rounding};

/// `ULtoQ`: pack the parser's words into the binary128 slot.
pub(crate) fn ultoq(bits: &[u32], expt: i32, k: i32) -> LongDouble {
    let mut l = [0u32; 4];
    match k & STRTOG_RETMASK {
        STRTOG_NONUMBER | STRTOG_ZERO => {}
        STRTOG_NORMAL | STRTOG_NANBITS => {
            l[0] = bits.first().copied().unwrap_or(0);
            l[1] = bits.get(1).copied().unwrap_or(0);
            l[2] = bits.get(2).copied().unwrap_or(0);
            l[3] = (bits.get(3).copied().unwrap_or(0) & !0x0001_0000)
                | (expt.wrapping_add(0x3fff).wrapping_add(112) as u32) << 16;
        }
        STRTOG_DENORMAL => {
            l[0] = bits.first().copied().unwrap_or(0);
            l[1] = bits.get(1).copied().unwrap_or(0);
            l[2] = bits.get(2).copied().unwrap_or(0);
            l[3] = bits.get(3).copied().unwrap_or(0);
        }
        STRTOG_INFINITE => {
            l[3] = 0x7fff_0000;
        }
        STRTOG_NAN => {
            l = LD_QNAN;
        }
        _ => {}
    }
    if k & STRTOG_NEG != 0 {
        l[3] |= 0x8000_0000;
    }
    let mut out = [0u8; 16];
    let mut i = 0usize;
    while i < 4 {
        let word = l[i].to_le_bytes();
        out[i * 4] = word[0];
        out[i * 4 + 1] = word[1];
        out[i * 4 + 2] = word[2];
        out[i * 4 + 3] = word[3];
        i += 1;
    }
    LongDouble(out)
}

/// `strtorQ`: parse with an explicit rounding direction.
pub(crate) fn strtor_q(
    ctx: &mut DtoaCtx,
    s: &[u8],
    loc: &NumericLocale,
    rounding: Rounding,
) -> Result<(LongDouble, usize, i32), DtoaError> {
    let fpi = Fpi::ld128().with_rounding(rounding.as_fpi());
    let mut bits = [0u32; 4];
    let mut expt = 0i32;
    let (consumed, k) = crate::strtodg::strtodg(ctx, s, &fpi, &mut expt, &mut bits, loc)?;
    Ok((ultoq(&bits, expt, k), consumed, k))
}

/// `strtopQ`: the entry `strtold` calls on aarch64 and riscv64.
#[allow(dead_code)] // aarch64/riscv64 entry; compiled on x86_64 for the ld128 tests
pub(crate) fn strtop_q(
    ctx: &mut DtoaCtx,
    s: &[u8],
    loc: &NumericLocale,
    rounding: Rounding,
) -> Result<(LongDouble, usize, i32), DtoaError> {
    strtor_q(ctx, s, loc, rounding)
}

#[cfg(test)]
#[path = "fmt_ld128_ut.rs"]
mod tests;
