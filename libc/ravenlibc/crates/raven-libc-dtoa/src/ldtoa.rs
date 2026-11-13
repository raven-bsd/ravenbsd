// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Copyright (c) 2003 David Schultz <das@FreeBSD.ORG>
// All rights reserved.
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/ldtoa.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: ldtoa.c,v 1.6 2019/08/01 02:27:43 riastradh Exp $)
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
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//! `ldtoa.c` of NetBSD gdtoa: C `long double` to decimal digits through the generic `gdtoa`.
//!
//! The donor hard-wires `FPI_Round_near` (`:57`); this port passes [`Rounding`] (A-num-9, a
//! recorded deviation). The `-32768 -> INT_MAX` sentinel mapping is gone: the public result is
//! [`crate::Kind`].

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::ctx::DtoaCtx;
use crate::types::*;
use crate::{Digits, DtoaError, LongDouble, Rounding};

/// Decompose an x87 80-bit image into `(be, bits, kind, negative)` for `gdtoa`.
#[cfg(any(target_arch = "x86_64", test))]
pub(crate) fn decompose_ld80(v: &LongDouble) -> (i32, [u32; 2], i32, bool) {
    let mut frac: u64 = 0;
    let mut i = 0usize;
    while i < 8 {
        frac |= u64::from(v.0[i]) << (i * 8);
        i += 1;
    }
    let se = u16::from(v.0[8]) | (u16::from(v.0[9]) << 8);
    let negative = se & 0x8000 != 0;
    let exp = i32::from(se & 0x7fff);
    let bits = [frac as u32, (frac >> 32) as u32];
    let mut be = exp.wrapping_sub(16383).wrapping_sub(63);
    let kind;
    if exp == 0 {
        if frac == 0 {
            kind = STRTOG_ZERO;
        } else if frac >> 63 == 0 {
            kind = STRTOG_DENORMAL;
            be = be.wrapping_add(1);
        } else {
            kind = STRTOG_NAN;
        }
    } else if exp == 0x7fff {
        if frac == 0x8000_0000_0000_0000 {
            kind = STRTOG_INFINITE;
        } else {
            kind = STRTOG_NAN;
        }
    } else if frac >> 63 == 0 {
        kind = STRTOG_NAN;
    } else {
        kind = STRTOG_NORMAL;
    }
    (be, bits, kind, negative)
}

/// Decompose an IEEE binary128 image into `(be, bits, kind, negative)` for `gdtoa`.
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64", test))]
pub(crate) fn decompose_ld128(v: &LongDouble) -> (i32, [u32; 4], i32, bool) {
    let raw = u128::from_le_bytes(v.0);
    let negative = raw >> 127 != 0;
    let exp = ((raw >> 112) & 0x7fff) as i32;
    let frac_mask: u128 = (1u128 << 112) - 1;
    let frac = raw & frac_mask;
    let mut be = exp.wrapping_sub(16383).wrapping_sub(112);
    let kind;
    if exp == 0 {
        if frac == 0 {
            kind = STRTOG_ZERO;
        } else {
            kind = STRTOG_DENORMAL;
            be = be.wrapping_add(1);
        }
    } else if exp == 0x7fff {
        if frac == 0 {
            kind = STRTOG_INFINITE;
        } else {
            kind = STRTOG_NAN;
        }
    } else {
        kind = STRTOG_NORMAL;
    }
    let mut bits = [0u32; 4];
    if kind == STRTOG_NORMAL {
        bits[3] |= 1 << 16;
    }
    let mut i = 0usize;
    while i < 4 {
        bits[i] |= ((frac >> (i * 32)) & 0xffff_ffff) as u32;
        i += 1;
    }
    (be, bits, kind, negative)
}

/// Finish a `gdtoa` result as [`Digits`].
#[allow(clippy::too_many_arguments)]
fn finish(
    ctx: &mut DtoaCtx,
    fpi: &Fpi,
    be: i32,
    bits: &[u32],
    kind: i32,
    negative: bool,
    mode: i32,
    ndigits: i32,
    rounding: Rounding,
    out: &mut [u8],
) -> Result<Digits, DtoaError> {
    let mut kindp = kind;
    if negative {
        kindp |= STRTOG_NEG;
    }
    let (len, decpt) = crate::gdtoa::gdtoa(
        ctx,
        &fpi.with_rounding(rounding.as_fpi()),
        be,
        bits,
        &mut kindp,
        mode,
        ndigits,
        rounding,
        out,
    )?;
    Ok(Digits {
        len,
        decpt,
        negative: kindp & STRTOG_NEG != 0,
        kind: crate::gdtoa::kind_of(kindp),
    })
}

/// `ldtoa`: long double to decimal digits in the caller's buffer.
pub(crate) fn ldtoa(
    ctx: &mut DtoaCtx,
    v: LongDouble,
    mode: i32,
    ndigits: i32,
    rounding: Rounding,
    out: &mut [u8],
) -> Result<Digits, DtoaError> {
    #[cfg(target_arch = "x86_64")]
    {
        let (be, bits, kind, negative) = decompose_ld80(&v);
        finish(
            ctx,
            &Fpi::ld80(),
            be,
            &bits,
            kind,
            negative,
            mode,
            ndigits,
            rounding,
            out,
        )
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        let (be, bits, kind, negative) = decompose_ld128(&v);
        finish(
            ctx,
            &Fpi::ld128(),
            be,
            &bits,
            kind,
            negative,
            mode,
            ndigits,
            rounding,
            out,
        )
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    {
        let _ = (ctx, v, mode, ndigits, rounding, out);
        Err(DtoaError::NoMemory)
    }
}

/// Test-only ld128 entry: on every host the binary128 property tests can drive `gdtoa` with
/// [`Fpi::ld128`] (5.4a).
#[cfg(test)]
pub(crate) fn ldtoa_ld128(
    ctx: &mut DtoaCtx,
    v: LongDouble,
    mode: i32,
    ndigits: i32,
    rounding: Rounding,
    out: &mut [u8],
) -> Result<Digits, DtoaError> {
    let (be, bits, kind, negative) = decompose_ld128(&v);
    finish(
        ctx,
        &Fpi::ld128(),
        be,
        &bits,
        kind,
        negative,
        mode,
        ndigits,
        rounding,
        out,
    )
}

#[cfg(test)]
#[path = "ldtoa_ut.rs"]
mod tests;
