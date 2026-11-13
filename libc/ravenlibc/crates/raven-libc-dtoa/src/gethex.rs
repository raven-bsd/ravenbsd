// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/gethex.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: gethex.c,v 1.7 2020/02/22 00:38:14 kamil Exp $)
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
//! `gethex.c` of NetBSD gdtoa: C99 hexadecimal floating-point constants.
//!
//! Transpiled from `netbsd:lib/libc/gdtoa/gethex.c`; the lazy `hexdig_init_D2A()` becomes the
//! compile-time `HEXDIG` table.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::config::{ALL_ON, KMASK, KSHIFT, UL_BITS, at};
use crate::ctx::{BigRef, DtoaCtx};
use crate::hexdig::HEXDIG;
use crate::types::*;
use crate::{DtoaError, NumericLocale};

/// `gethex`: parse `0x...p...` from `input[sp]`, write the result block to `bp` and the exponent
/// to `expt`; return the `STRTOG_*` flags.
#[allow(clippy::too_many_arguments)] // donor argument list
pub(crate) fn gethex(
    ctx: &mut DtoaCtx,
    input: &[u8],
    sp: &mut usize,
    fpi: &Fpi,
    expt: &mut i32,
    bp: &mut BigRef,
    sign: i32,
    loc: &NumericLocale,
) -> Result<i32, DtoaError> {
    let mut s;
    let mut s0;
    let mut decpt: Option<usize> = None;
    let mut havedig = 0i32;
    let mut zret = false;
    let mut e = 0i32;
    let dplen = loc.decimal_point.len();
    *bp = BigRef::NIL;

    s0 = sp.wrapping_add(2);
    // The donor's `havedig` counts the leading zeros here and only later becomes the
    // "significant digit seen" flag; the count decides whether a bare zero consumes the digit.
    while at(input, s0) == b'0' {
        havedig = havedig.wrapping_add(1);
        s0 = s0.wrapping_add(1);
    }
    s = s0;
    let mut goto_pcheck = false;
    if HEXDIG[usize::from(at(input, s))] != 0 {
        havedig = 1;
    } else {
        zret = true;
        let mut i = 0usize;
        while i < dplen {
            if at(input, s.wrapping_add(i)) != loc.decimal_point[i] {
                goto_pcheck = true;
                break;
            }
            i = i.wrapping_add(1);
        }
        if !goto_pcheck {
            s = s.wrapping_add(i);
            decpt = Some(s);
            if HEXDIG[usize::from(at(input, s))] == 0 {
                goto_pcheck = true;
            } else {
                while at(input, s) == b'0' {
                    s = s.wrapping_add(1);
                }
                if HEXDIG[usize::from(at(input, s))] != 0 {
                    zret = false;
                }
                havedig = 1;
                s0 = s;
            }
        }
    }
    if !goto_pcheck {
        while HEXDIG[usize::from(at(input, s))] != 0 {
            s = s.wrapping_add(1);
        }
        if decpt.is_none() && at(input, s) == loc.decimal_point.first().copied().unwrap_or(b'.') {
            let mut i = 1usize;
            let mut matched = true;
            while i < dplen {
                if at(input, s.wrapping_add(i)) != loc.decimal_point[i] {
                    matched = false;
                    break;
                }
                i = i.wrapping_add(1);
            }
            if matched {
                s = s.wrapping_add(i);
                decpt = Some(s);
                while HEXDIG[usize::from(at(input, s))] != 0 {
                    s = s.wrapping_add(1);
                }
            }
        }
    }
    if let Some(d) = decpt {
        e = ((s.wrapping_sub(d)) as i32).wrapping_shl(2).wrapping_neg();
    }
    let s1 = s;
    let mut big = false;
    let mut esign = 0i32;
    let c = at(input, s);
    if c == b'p' || c == b'P' {
        s = s.wrapping_add(1);
        let c = at(input, s);
        match c {
            b'-' => {
                esign = 1;
                s = s.wrapping_add(1);
            }
            b'+' => {
                s = s.wrapping_add(1);
            }
            _ => {}
        }
        let n = HEXDIG[usize::from(at(input, s))];
        if n == 0 || n > 0x19 {
            s = s1;
        } else {
            let mut e1 = i32::from(n).wrapping_sub(0x10);
            loop {
                s = s.wrapping_add(1);
                let n = HEXDIG[usize::from(at(input, s))];
                if n == 0 || n > 0x19 {
                    break;
                }
                if e1 & 0xf800_0000u32 as i32 != 0 {
                    big = true;
                }
                e1 = e1
                    .wrapping_mul(10)
                    .wrapping_add(i32::from(n).wrapping_sub(0x10));
            }
            if esign != 0 {
                e1 = e1.wrapping_neg();
            }
            e = e.wrapping_add(e1);
        }
    }
    *sp = s;
    if havedig == 0 {
        *sp = s0.wrapping_sub(1);
    }
    if zret {
        return Ok(STRTOG_ZERO);
    }
    if big {
        if esign != 0 {
            let tiny = match fpi.rounding {
                FPI_ROUND_UP => sign == 0,
                FPI_ROUND_DOWN => sign != 0,
                _ => false,
            };
            if tiny {
                let b = ctx.balloc(0).ok_or(DtoaError::NoMemory)?;
                ctx.set_wds(b, 1);
                ctx.set_x(b, 0, 1);
                *bp = b;
                *expt = fpi.emin;
                return Ok(STRTOG_DENORMAL | STRTOG_INEXHI | STRTOG_UNDERFLOW);
            }
            return Ok(STRTOG_ZERO | STRTOG_INEXLO | STRTOG_UNDERFLOW);
        }
        let ovfl = match fpi.rounding {
            FPI_ROUND_NEAR => true,
            FPI_ROUND_UP => sign == 0,
            FPI_ROUND_DOWN => sign != 0,
            _ => false,
        };
        if ovfl {
            return Ok(STRTOG_INFINITE | STRTOG_OVERFLOW | STRTOG_INEXHI);
        }
        // ret_big
        let nbits = fpi.nbits;
        let n0 = (nbits as u32 >> KSHIFT) as i32;
        let mut n = n0;
        if nbits & KMASK as i32 != 0 {
            n = n.wrapping_add(1);
        }
        let mut k = 0i32;
        let mut j = n;
        loop {
            j = (j as u32 >> 1) as i32;
            if j == 0 {
                break;
            }
            k = k.wrapping_add(1);
        }
        let b = ctx.balloc(k).ok_or(DtoaError::NoMemory)?;
        ctx.set_wds(b, n);
        let mut j = 0i32;
        while j < n0 {
            ctx.set_x(b, j as u32, ALL_ON);
            j = j.wrapping_add(1);
        }
        if n > n0 {
            ctx.set_x(
                b,
                j as u32,
                UL_BITS >> (UL_BITS.wrapping_sub((nbits & KMASK as i32) as u32)),
            );
        }
        *bp = b;
        *expt = fpi.emin;
        return Ok(STRTOG_NORMAL | STRTOG_INEXLO);
    }
    let mut n = (s1.wrapping_sub(s0) as i32).wrapping_sub(1);
    let mut k = 0i32;
    while n > (1i32 << (KSHIFT.wrapping_sub(2))).wrapping_sub(1) {
        n = (n as u32 >> 1) as i32;
        k = k.wrapping_add(1);
    }
    let mut b = ctx.balloc(k).ok_or(DtoaError::NoMemory)?;
    let mut x = 0u32;
    n = 0;
    let mut l: u32 = 0;
    let mut i = 0usize;
    while i.wrapping_add(1) < dplen {
        i = i.wrapping_add(1);
    }
    let mut s1 = s1;
    while s1 > s0 {
        s1 = s1.wrapping_sub(1);
        if at(input, s1) == loc.decimal_point[i] {
            s1 = s1.wrapping_sub(i);
            continue;
        }
        if n == UL_BITS as i32 {
            ctx.set_x(b, x, l);
            x = x.wrapping_add(1);
            l = 0;
            n = 0;
        }
        l |= u32::from(HEXDIG[usize::from(at(input, s1))] & 0x0f) << (n as u32);
        n = n.wrapping_add(4);
    }
    ctx.set_x(b, x, l);
    x = x.wrapping_add(1);
    n = x as i32;
    ctx.set_wds(b, n);
    n = UL_BITS
        .wrapping_mul(n as u32)
        .wrapping_sub(crate::bigint::hi0bits(l) as u32) as i32;
    let nbits = fpi.nbits;
    let mut lostbits = 0u32;
    if n > nbits {
        n = n.wrapping_sub(nbits);
        if crate::bigint::any_on(ctx, b, n) != 0 {
            lostbits = 1;
            let k2 = n.wrapping_sub(1);
            if ctx.x(b, (k2 as u32) >> KSHIFT) & (1u32 << (k2 & KMASK as i32)) != 0 {
                lostbits = 2;
                if k2 > 0 && crate::bigint::any_on(ctx, b, k2) != 0 {
                    lostbits = 3;
                }
            }
        }
        crate::bigint::rshift(ctx, b, n);
        e = e.wrapping_add(n);
    } else if n < nbits {
        n = nbits.wrapping_sub(n);
        b = crate::bigint::lshift(ctx, b, n).ok_or(DtoaError::NoMemory)?;
        e = e.wrapping_sub(n);
    }
    if e > fpi.emax {
        ctx.bfree(b);
        return Ok(STRTOG_INFINITE | STRTOG_OVERFLOW | STRTOG_INEXHI);
    }
    let mut irv = STRTOG_NORMAL;
    if e < fpi.emin {
        irv = STRTOG_DENORMAL;
        n = fpi.emin.wrapping_sub(e);
        if n >= nbits {
            let one_bit = match fpi.rounding {
                FPI_ROUND_NEAR => {
                    n == nbits && (n < 2 || crate::bigint::any_on(ctx, b, n.wrapping_sub(1)) != 0)
                }
                FPI_ROUND_UP => sign == 0,
                FPI_ROUND_DOWN => sign != 0,
                _ => false,
            };
            if one_bit {
                ctx.set_x(b, 0, 1);
                ctx.set_wds(b, 1);
                *bp = b;
                *expt = fpi.emin;
                return Ok(STRTOG_DENORMAL | STRTOG_INEXHI | STRTOG_UNDERFLOW);
            }
            ctx.bfree(b);
            return Ok(STRTOG_ZERO | STRTOG_INEXLO | STRTOG_UNDERFLOW);
        }
        let k2 = n.wrapping_sub(1);
        if lostbits != 0 {
            lostbits = 1;
        } else if k2 > 0 {
            lostbits = crate::bigint::any_on(ctx, b, k2);
        }
        if ctx.x(b, (k2 as u32) >> KSHIFT) & (1u32 << (k2 & KMASK as i32)) != 0 {
            lostbits |= 2;
        }
        crate::bigint::rshift(ctx, b, n);
        e = fpi.emin;
    }
    if lostbits != 0 {
        let mut up = false;
        match fpi.rounding {
            FPI_ROUND_ZERO => {}
            FPI_ROUND_NEAR => {
                if lostbits & 2 != 0 && (lostbits | ctx.x(b, 0)) & 1 != 0 {
                    up = true;
                }
            }
            FPI_ROUND_UP => up = sign == 0,
            FPI_ROUND_DOWN => up = sign != 0,
            _ => {}
        }
        if up {
            let k2 = ctx.wds(b);
            b = crate::strtodg::increment(ctx, b).ok_or(DtoaError::NoMemory)?;
            if irv == STRTOG_DENORMAL {
                if nbits == fpi.nbits.wrapping_sub(1)
                    && ctx.x(b, (nbits as u32) >> KSHIFT) & (1u32 << (nbits & KMASK as i32)) != 0
                {
                    irv = STRTOG_NORMAL;
                }
            } else if ctx.wds(b) > k2
                || ((nbits & KMASK as i32) != 0
                    && crate::bigint::hi0bits(ctx.x(b, (k2 - 1) as u32))
                        < (32i32.wrapping_sub(nbits & KMASK as i32)))
            {
                crate::bigint::rshift(ctx, b, 1);
                e = e.wrapping_add(1);
                if e > fpi.emax {
                    ctx.bfree(b);
                    return Ok(STRTOG_INFINITE | STRTOG_OVERFLOW | STRTOG_INEXHI);
                }
            }
            irv |= STRTOG_INEXHI;
        } else {
            irv |= STRTOG_INEXLO;
        }
    }
    *bp = b;
    *expt = e;
    Ok(irv)
}

#[cfg(test)]
#[path = "gethex_ut.rs"]
mod tests;
