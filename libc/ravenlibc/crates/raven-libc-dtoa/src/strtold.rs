// SPDX-License-Identifier: LicenseRef-PublicDomain
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 2026 The NetBSD Foundation, Inc.
// Public domain.
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/strtold_subr.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: strtold_subr.c,v 1.3 2013/05/17 12:55:57 joerg Exp $)
//
// Written by Klaus Klein <kleink@NetBSD.org>, November 16, 2005.
//
// The donor is a format-selecting include helper (`strtold_px.c` / `strtold_pQ.c`); the selection
// is a `cfg`, not an `#ifdef` (A-num-15).
//! `strtold`: x87 80-bit on x86_64, IEEE binary128 on aarch64 and riscv64.

use crate::ctx::DtoaCtx;
use crate::{DtoaError, LongDouble, NumericLocale, Rounding};

/// `strtold`: parse into the platform's C `long double` memory image.
pub(crate) fn strtold(
    ctx: &mut DtoaCtx,
    s: &[u8],
    loc: &NumericLocale,
    rounding: Rounding,
) -> Result<(LongDouble, usize, i32), DtoaError> {
    #[cfg(target_arch = "x86_64")]
    {
        crate::fmt_ld80::strtopx(ctx, s, loc, rounding)
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        crate::fmt_ld128::strtop_q(ctx, s, loc, rounding)
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    {
        let _ = (ctx, s, loc, rounding);
        Err(DtoaError::NoMemory)
    }
}

#[cfg(test)]
#[path = "strtold_ut.rs"]
mod tests;
