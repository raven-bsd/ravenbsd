// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998-2001 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/strtodg.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: strtodg.c,v 1.13 2021/05/06 16:15:33 christos Exp $)
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
//! `strtodg.c` of NetBSD gdtoa: decimal/hex/inf/nan to a binary format described by [`Fpi`].
//!
//! The correctly rounded generic parser used by `strtof` (binary32), `strtold` (ld80/ld128) and the
//! explicit-rounding `strtord`; `strtod.c` keeps its own dedicated double path (A-num-1). Every
//! `STRTOG_NoMemory` return becomes [`DtoaError::NoMemory`].

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::config::{
    ALL_ON, BIAS, BIGTENS, DBL_DIG, EXP_MSK1, EXP_SHIFT1, KMASK, KSHIFT, N_BIGTENS, P, TEN_PMAX,
    TENS, TINYTENS, UL_BITS, at, w0_and, w0_or, word0, word1,
};
use crate::ctx::{BigRef, DtoaCtx};
use crate::types::*;
use crate::{DtoaError, NumericLocale};

/// The donor's `fivesbits[]`.
const FIVESBITS: [i32; 23] = [
    0, 3, 5, 7, 10, 12, 14, 17, 19, 21, 24, 26, 28, 31, 33, 35, 38, 40, 42, 45, 47, 49, 52,
];

/// `increment(b)`.
pub(crate) fn increment(ctx: &mut DtoaCtx, b: BigRef) -> Option<BigRef> {
    let mut b = b;
    let wds = ctx.wds(b);
    let mut x = 0i32;
    loop {
        if ctx.x(b, x as u32) < 0xffff_ffff {
            let value = ctx.x(b, x as u32).wrapping_add(1);
            ctx.set_x(b, x as u32, value);
            return Some(b);
        }
        ctx.set_x(b, x as u32, 0);
        x = x.wrapping_add(1);
        if x >= wds {
            break;
        }
    }
    if ctx.wds(b) >= ctx.maxwds(b) {
        let b1 = ctx.balloc(ctx.k(b).wrapping_add(1))?;
        crate::bigint::bcopy(ctx, b1, b);
        ctx.bfree(b);
        b = b1;
    }
    let wds = ctx.wds(b);
    ctx.set_x(b, wds as u32, 1);
    ctx.set_wds(b, wds.wrapping_add(1));
    Some(b)
}

/// `decrement(b)`.
pub(crate) fn decrement(ctx: &mut DtoaCtx, b: BigRef) {
    let wds = ctx.wds(b);
    let mut x = 0i32;
    loop {
        if ctx.x(b, x as u32) != 0 {
            let value = ctx.x(b, x as u32).wrapping_sub(1);
            ctx.set_x(b, x as u32, value);
            break;
        }
        ctx.set_x(b, x as u32, 0xffff_ffff);
        x = x.wrapping_add(1);
        if x >= wds {
            break;
        }
    }
}

/// `all_on(b, n)`.
pub(crate) fn all_on(ctx: &DtoaCtx, b: BigRef, n: i32) -> bool {
    let mut n = n;
    let xe = (n as u32) >> KSHIFT;
    let mut x = 0u32;
    while x < xe {
        if ctx.x(b, x) & ALL_ON != ALL_ON {
            return false;
        }
        x = x.wrapping_add(1);
    }
    n &= KMASK as i32;
    if n != 0 {
        return ((ctx.x(b, x) | (ALL_ON << (n as u32))) & ALL_ON) == ALL_ON;
    }
    true
}

/// `set_ones(b, n)`.
pub(crate) fn set_ones(ctx: &mut DtoaCtx, b: BigRef, n: i32) -> Option<BigRef> {
    let k = ((n.wrapping_add((1i32 << KSHIFT).wrapping_sub(1))) as u32) >> KSHIFT;
    let mut b = b;
    if (ctx.k(b) as u32) < k {
        ctx.bfree(b);
        b = ctx.balloc(k as i32)?;
    }
    let mut k = (n as u32) >> KSHIFT;
    if n & KMASK as i32 != 0 {
        k = k.wrapping_add(1);
    }
    ctx.set_wds(b, k as i32);
    let mut x = 0u32;
    while x < k {
        ctx.set_x(b, x, ALL_ON);
        x = x.wrapping_add(1);
    }
    if n & KMASK as i32 != 0 {
        let shift = UL_BITS.wrapping_sub((n & KMASK as i32) as u32);
        let value = ctx.x(b, k.wrapping_sub(1)) >> shift;
        ctx.set_x(b, k.wrapping_sub(1), value);
    }
    Some(b)
}

/// `rvOK`: whether the current `double` approximation is good enough.
#[allow(clippy::too_many_arguments, unused_assignments)]
fn rv_ok(
    ctx: &mut DtoaCtx,
    d: f64,
    fpi: &Fpi,
    expt: &mut i32,
    bits: &mut [u32],
    exact: bool,
    rd: i32,
    irv: &mut i32,
) -> Result<bool, DtoaError> {
    let mut carry = 0u32;
    let mut e = 0i32;
    let mut bdif = 0i32;
    let mut b = crate::bigint::d2b(ctx, d, &mut e, &mut bdif).ok_or(DtoaError::NoMemory)?;
    let nb = fpi.nbits;
    bdif = bdif.wrapping_sub(nb);
    e = e.wrapping_add(bdif);
    if bdif <= 0 {
        if exact {
            // trunc
        } else {
            ctx.bfree(b);
            return Ok(false);
        }
    } else if P == nb {
        if exact && fpi.rounding == FPI_ROUND_NEAR {
            // trunc
        } else {
            ctx.bfree(b);
            return Ok(false);
        }
    } else {
        match rd {
            1 => {}
            2 => carry = 1,
            _ => {
                let k = bdif.wrapping_sub(1);
                if k == 0 {
                    if !exact {
                        ctx.bfree(b);
                        return Ok(false);
                    }
                    if ctx.x(b, 0) & 2 != 0 {
                        carry = 1;
                    }
                } else if ctx.x(b, (k as u32) >> KSHIFT) & (1u32 << (k & KMASK as i32)) != 0 {
                    carry = 1;
                }
            }
        }
    }
    let mut inex = 0i32;
    let mut lostbits = 0u32;
    if bdif > 0 {
        lostbits = crate::bigint::any_on(ctx, b, bdif);
        if lostbits != 0 {
            inex = STRTOG_INEXLO;
        }
        crate::bigint::rshift(ctx, b, bdif);
        if carry != 0 {
            inex = STRTOG_INEXHI;
            b = increment(ctx, b).ok_or(DtoaError::NoMemory)?;
            let mut j = nb & KMASK as i32;
            if j != 0 {
                j = (UL_BITS as i32).wrapping_sub(j);
            }
            if crate::bigint::hi0bits(ctx.x(b, (ctx.wds(b) - 1) as u32)) != j {
                if lostbits == 0 {
                    lostbits = ctx.x(b, 0) & 1;
                }
                crate::bigint::rshift(ctx, b, 1);
                e = e.wrapping_add(1);
            }
        }
    } else if bdif < 0 {
        b = crate::bigint::lshift(ctx, b, bdif.wrapping_neg()).ok_or(DtoaError::NoMemory)?;
    }
    if e < fpi.emin {
        let k = fpi.emin.wrapping_sub(e);
        e = fpi.emin;
        if k > nb || fpi.sudden_underflow != 0 {
            ctx.set_wds(b, 0);
            inex = 0;
            *irv = STRTOG_UNDERFLOW | STRTOG_INEXLO;
        } else {
            let k1 = k.wrapping_sub(1);
            if k1 > 0 && lostbits == 0 {
                lostbits = crate::bigint::any_on(ctx, b, k1);
            }
            if lostbits == 0 && !exact {
                ctx.bfree(b);
                return Ok(false);
            }
            let carry2 = ctx.x(b, (k1 as u32) >> KSHIFT) & (1u32 << (k1 & KMASK as i32));
            lostbits |= carry2;
            crate::bigint::rshift(ctx, b, k);
            *irv = STRTOG_DENORMAL;
            if carry2 != 0 {
                b = increment(ctx, b).ok_or(DtoaError::NoMemory)?;
                inex = STRTOG_INEXHI | STRTOG_UNDERFLOW;
            } else if lostbits != 0 {
                inex = STRTOG_INEXLO | STRTOG_UNDERFLOW;
            }
        }
    } else if e > fpi.emax {
        e = fpi.emax.wrapping_add(1);
        *irv = STRTOG_INFINITE | STRTOG_OVERFLOW | STRTOG_INEXHI;
        ctx.set_wds(b, 0);
        inex = 0;
    }
    *expt = e;
    crate::bigint::copybits(bits, nb, b, ctx);
    *irv |= inex;
    ctx.bfree(b);
    Ok(true)
}

/// `mantbits(d)`.
fn mantbits(d: f64) -> i32 {
    let mut l = word1(d);
    if l != 0 {
        return P.wrapping_sub(crate::bigint::lo0bits(&mut l));
    }
    l = word0(d) | EXP_MSK1;
    P.wrapping_sub(32)
        .wrapping_sub(crate::bigint::lo0bits(&mut l))
}

/// `strtodg`: the generic correctly rounded parser.
#[allow(unused_assignments, clippy::collapsible_if, clippy::collapsible_match)] // donor initialisations and nested special-case tests
pub(crate) fn strtodg(
    ctx: &mut DtoaCtx,
    input: &[u8],
    fpi: &Fpi,
    expt: &mut i32,
    bits: &mut [u32],
    loc: &NumericLocale,
) -> Result<(usize, i32), DtoaError> {
    let input_start = 0usize;
    let mut s = 0usize;
    let mut sign = 0i32;
    let mut irv = STRTOG_ZERO;
    let mut denorm = 0i32;
    let mut nz0 = 0i32;
    let mut nz = 0i32;
    let mut rv: f64 = 0.0;
    let mut rvb = BigRef::NIL;
    let nbits = fpi.nbits;

    let mut skip = true;
    while skip {
        let c = at(input, s);
        match c {
            b'-' => {
                sign = 1;
                s = s.wrapping_add(1);
                if at(input, s) != 0 {
                    skip = false;
                } else {
                    sign = 0;
                    irv = STRTOG_NONUMBER;
                    s = input_start;
                    return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
                }
            }
            b'+' => {
                s = s.wrapping_add(1);
                if at(input, s) != 0 {
                    skip = false;
                } else {
                    sign = 0;
                    irv = STRTOG_NONUMBER;
                    s = input_start;
                    return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
                }
            }
            0 => {
                sign = 0;
                irv = STRTOG_NONUMBER;
                s = input_start;
                return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
            }
            b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | b' ' => {
                s = s.wrapping_add(1);
            }
            _ => skip = false,
        }
    }

    if at(input, s) == b'0' {
        match at(input, s.wrapping_add(1)) {
            b'x' | b'X' => {
                irv = crate::gethex::gethex(ctx, input, &mut s, fpi, expt, &mut rvb, sign, loc)?;
                if irv == STRTOG_NONUMBER {
                    s = input_start;
                    sign = 0;
                }
                return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
            }
            _ => {}
        }
        nz0 = 1;
        s = s.wrapping_add(1);
        while at(input, s) == b'0' {
            s = s.wrapping_add(1);
        }
        if at(input, s) == 0 {
            return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
        }
    }
    let mut sudden_underflow = fpi.sudden_underflow;
    let mut s0 = s;
    let mut y: u32 = 0;
    let mut z: u32 = 0;
    let mut decpt = 0i32;
    let mut nd = 0i32;
    let mut nf = 0i32;
    let mut c = at(input, s);
    while c.is_ascii_digit() {
        if nd < 9 {
            y = y.wrapping_mul(10).wrapping_add(u32::from(c - b'0'));
        } else if nd < 16 {
            z = z.wrapping_mul(10).wrapping_add(u32::from(c - b'0'));
        }
        nd = nd.wrapping_add(1);
        s = s.wrapping_add(1);
        c = at(input, s);
    }
    let nd0 = nd;
    let dplen = loc.decimal_point.len();
    let mut have_dig = false;
    let dot = loc.decimal_point.first().copied().unwrap_or(b'.');
    if c == dot {
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
            c = at(input, s);
            decpt = 1;
            if nd == 0 {
                while c == b'0' {
                    nz = nz.wrapping_add(1);
                    s = s.wrapping_add(1);
                    c = at(input, s);
                }
                if c > b'0' && c <= b'9' {
                    s0 = s;
                    nf = nf.wrapping_add(nz);
                    nz = 0;
                    have_dig = true;
                }
            } else {
                have_dig = true;
            }
            if have_dig {
                let mut first = true;
                loop {
                    if !first && !c.is_ascii_digit() {
                        break;
                    }
                    first = false;
                    nz = nz.wrapping_add(1);
                    let digit = u32::from(c.wrapping_sub(b'0'));
                    if digit != 0 {
                        nf = nf.wrapping_add(nz);
                        let mut i = 1i32;
                        while i < nz {
                            // The donor's `if (nd++ < 9) ... else if (nd <= DBL_DIG+1) ...`
                            // increments before the second test; a pre-increment here would let
                            // a 17th fractional digit into `z` and break the k/scaling pairing.
                            nd = nd.wrapping_add(1);
                            if nd <= 9 {
                                y = y.wrapping_mul(10);
                            } else if nd <= DBL_DIG + 1 {
                                z = z.wrapping_mul(10);
                            }
                            i = i.wrapping_add(1);
                        }
                        nd = nd.wrapping_add(1);
                        if nd <= 9 {
                            y = y.wrapping_mul(10).wrapping_add(digit);
                        } else if nd <= DBL_DIG + 1 {
                            z = z.wrapping_mul(10).wrapping_add(digit);
                        }
                        nz = 0;
                    }
                    s = s.wrapping_add(1);
                    c = at(input, s);
                }
            }
        }
    }
    let mut e = 0i32;
    if c == b'e' || c == b'E' {
        if nd == 0 && nz == 0 && nz0 == 0 {
            irv = STRTOG_NONUMBER;
            s = input_start;
            return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
        }
        let s00 = s;
        let mut esign = 0i32;
        s = s.wrapping_add(1);
        c = at(input, s);
        match c {
            b'-' => {
                esign = 1;
                s = s.wrapping_add(1);
                c = at(input, s);
            }
            b'+' => {
                s = s.wrapping_add(1);
                c = at(input, s);
            }
            _ => {}
        }
        if c.is_ascii_digit() {
            while c == b'0' {
                s = s.wrapping_add(1);
                c = at(input, s);
            }
            if c > b'0' && c <= b'9' {
                let mut l = i64::from(c - b'0');
                let s1 = s;
                loop {
                    s = s.wrapping_add(1);
                    c = at(input, s);
                    if !c.is_ascii_digit() {
                        break;
                    }
                    l = l.wrapping_mul(10).wrapping_add(i64::from(c - b'0'));
                }
                if s.wrapping_sub(s1) > 8 || l > 19999 {
                    e = 19999;
                } else {
                    e = l as i32;
                }
                if esign != 0 {
                    e = e.wrapping_neg();
                }
            } else {
                e = 0;
            }
        } else {
            s = s00;
        }
    }
    if nd == 0 {
        if nz == 0 && nz0 == 0 {
            let mut infnan = false;
            if decpt == 0 {
                match c {
                    b'i' | b'I' => {
                        if crate::bigint::match_prefix(input, &mut s, b"nf") {
                            s = s.wrapping_sub(1);
                            if !crate::bigint::match_prefix(input, &mut s, b"inity") {
                                s = s.wrapping_add(1);
                            }
                            irv = STRTOG_INFINITE;
                            *expt = fpi.emax.wrapping_add(1);
                            infnan = true;
                        }
                    }
                    b'n' | b'N' => {
                        if crate::bigint::match_prefix(input, &mut s, b"an") {
                            irv = STRTOG_NAN;
                            *expt = fpi.emax.wrapping_add(1);
                            if at(input, s) == b'(' {
                                irv = crate::hexnan::hexnan(input, &mut s, fpi, bits);
                            }
                            *expt = fpi.emax.wrapping_add(1);
                            infnan = true;
                        }
                    }
                    _ => {}
                }
            }
            if !infnan {
                irv = STRTOG_NONUMBER;
                s = input_start;
            }
        }
        return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
    }

    irv = STRTOG_NORMAL;
    let mut e1 = e.wrapping_sub(nf);
    e = e1;
    let mut rd = 0i32;
    match fpi.rounding & 3 {
        FPI_ROUND_UP => rd = 2i32.wrapping_sub(sign),
        FPI_ROUND_ZERO => rd = 1,
        FPI_ROUND_DOWN => rd = 1i32.wrapping_add(sign),
        _ => {}
    }
    // `s2b` needs the original digit count before the decimal point: when there is no integer
    // digit, `s0` points at the first significant fractional digit and the separator is already
    // behind it, so no separator bytes must be skipped. The donor overwrites `nd0` with `nd`
    // afterwards, which loses that distinction.
    let s2b_dplen = if nd0 == 0 { 0 } else { dplen };
    let nd0 = if nd0 == 0 { nd } else { nd0 };
    let k = if nd < DBL_DIG + 1 { nd } else { DBL_DIG + 1 };
    rv = f64::from(y);
    if k > 9 {
        rv = TENS[(k - 9) as usize] * rv + f64::from(z);
    }
    let mut bd0 = BigRef::NIL;
    let mut e2 = 0i32;
    if nbits <= P && nd <= DBL_DIG {
        if e == 0 {
            if rv_ok(ctx, rv, fpi, expt, bits, true, rd, &mut irv)? {
                return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
            }
        } else if e > 0 {
            if e <= TEN_PMAX {
                let i = FIVESBITS[e as usize].wrapping_add(mantbits(rv)) <= P;
                rv *= TENS[e as usize];
                if rv_ok(ctx, rv, fpi, expt, bits, i, rd, &mut irv)? {
                    return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
                }
                e1 = e1.wrapping_sub(e);
            } else {
                let i = DBL_DIG.wrapping_sub(nd);
                if e <= TEN_PMAX.wrapping_add(i) {
                    e2 = e.wrapping_sub(i);
                    e1 = e1.wrapping_sub(i);
                    rv *= TENS[i as usize];
                    rv *= TENS[e2 as usize];
                    if rv_ok(ctx, rv, fpi, expt, bits, false, rd, &mut irv)? {
                        return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
                    }
                    e1 = e1.wrapping_sub(e2);
                }
            }
        } else if e >= TEN_PMAX.wrapping_neg() {
            rv /= TENS[e.wrapping_neg() as usize];
            if rv_ok(ctx, rv, fpi, expt, bits, false, rd, &mut irv)? {
                return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
            }
            e1 = e1.wrapping_sub(e);
        }
    }
    // rv_notOK
    e1 = e1.wrapping_add(nd.wrapping_sub(k));
    e2 = 0;
    if e1 > 0 {
        let i = e1 & 15;
        if i != 0 {
            rv *= TENS[i as usize];
        }
        e1 &= !15;
        if e1 != 0 {
            e1 = (e1 as u32 >> 4) as i32;
            while e1 >= (1i32 << (N_BIGTENS - 1)) {
                e2 = e2.wrapping_add(
                    (((word0(rv) & crate::config::EXP_MASK) >> EXP_SHIFT1) as i32)
                        .wrapping_sub(BIAS),
                );
                w0_and(&mut rv, !crate::config::EXP_MASK);
                w0_or(&mut rv, (BIAS as u32) << EXP_SHIFT1);
                rv *= BIGTENS[N_BIGTENS - 1];
                e1 = e1.wrapping_sub(1i32 << (N_BIGTENS - 1));
            }
            e2 = e2.wrapping_add(
                (((word0(rv) & crate::config::EXP_MASK) >> EXP_SHIFT1) as i32).wrapping_sub(BIAS),
            );
            w0_and(&mut rv, !crate::config::EXP_MASK);
            w0_or(&mut rv, (BIAS as u32) << EXP_SHIFT1);
            let mut j = 0usize;
            while e1 > 0 {
                if e1 & 1 != 0 {
                    rv *= BIGTENS[j];
                }
                e1 = (e1 as u32 >> 1) as i32;
                j = j.wrapping_add(1);
            }
        }
    } else if e1 < 0 {
        e1 = e1.wrapping_neg();
        let i = e1 & 15;
        if i != 0 {
            rv /= TENS[i as usize];
        }
        e1 &= !15;
        if e1 != 0 {
            e1 = (e1 as u32 >> 4) as i32;
            while e1 >= (1i32 << (N_BIGTENS - 1)) {
                e2 = e2.wrapping_add(
                    (((word0(rv) & crate::config::EXP_MASK) >> EXP_SHIFT1) as i32)
                        .wrapping_sub(BIAS),
                );
                w0_and(&mut rv, !crate::config::EXP_MASK);
                w0_or(&mut rv, (BIAS as u32) << EXP_SHIFT1);
                rv *= TINYTENS[N_BIGTENS - 1];
                e1 = e1.wrapping_sub(1i32 << (N_BIGTENS - 1));
            }
            e2 = e2.wrapping_add(
                (((word0(rv) & crate::config::EXP_MASK) >> EXP_SHIFT1) as i32).wrapping_sub(BIAS),
            );
            w0_and(&mut rv, !crate::config::EXP_MASK);
            w0_or(&mut rv, (BIAS as u32) << EXP_SHIFT1);
            let mut j = 0usize;
            while e1 > 0 {
                if e1 & 1 != 0 {
                    rv *= TINYTENS[j];
                }
                e1 = (e1 as u32 >> 1) as i32;
                j = j.wrapping_add(1);
            }
        }
    }
    let mut rve = 0i32;
    let mut rvbits = 0i32;
    rvb = crate::bigint::d2b(ctx, rv, &mut rve, &mut rvbits).ok_or(DtoaError::NoMemory)?;
    rve = rve.wrapping_add(e2);
    let j = rvbits.wrapping_sub(nbits);
    if j > 0 {
        crate::bigint::rshift(ctx, rvb, j);
        rvbits = nbits;
        rve = rve.wrapping_add(j);
    }
    let mut bb0 = 0i32;
    e2 = rve.wrapping_add(rvbits).wrapping_sub(nbits);
    if e2 > fpi.emax.wrapping_add(1) {
        ctx.set_wds(rvb, 0);
        irv = STRTOG_INFINITE | STRTOG_OVERFLOW | STRTOG_INEXHI;
        *expt = fpi.emax.wrapping_add(1);
        return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
    }
    let mut rve1 = rve.wrapping_add(rvbits).wrapping_sub(nbits);
    let emin = fpi.emin;
    if e2 < emin {
        denorm = 1;
        let j = rve.wrapping_sub(emin);
        if j > 0 {
            rvb = crate::bigint::lshift(ctx, rvb, j).ok_or(DtoaError::NoMemory)?;
            rvbits = rvbits.wrapping_add(j);
        } else if j < 0 {
            rvbits = rvbits.wrapping_add(j);
            if rvbits <= 0 {
                if rvbits < -1 {
                    ctx.set_wds(rvb, 0);
                    ctx.set_x(rvb, 0, 0);
                    *expt = emin;
                    irv = STRTOG_UNDERFLOW | STRTOG_INEXLO;
                    return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
                }
                ctx.set_x(rvb, 0, 1);
                ctx.set_wds(rvb, 1);
                rvbits = 1;
            } else {
                crate::bigint::rshift(ctx, rvb, j.wrapping_neg());
            }
        }
        rve = emin;
        rve1 = emin;
        if sudden_underflow != 0 && e2.wrapping_add(1) < emin {
            ctx.set_wds(rvb, 0);
            ctx.set_x(rvb, 0, 0);
            *expt = emin;
            irv = STRTOG_UNDERFLOW | STRTOG_INEXLO;
            return ret(s, sign, irv, denorm, 0, rvb, bits, nbits, ctx);
        }
    }

    // The donor's s2b() skips `dplen` bytes after the first nine digits, assuming the decimal
    // point lies inside that group. When the number starts with a run of zeros, `s0` points at the
    // first significant fractional digit and the separator is already behind it, so the skip would
    // eat a real digit; `s2b_dplen` is 0 in that case.
    bd0 =
        crate::bigint::s2b(ctx, &input[s0..], nd0, nd, y, s2b_dplen).ok_or(DtoaError::NoMemory)?;
    let mut bb = BigRef::NIL;
    let mut bd = BigRef::NIL;
    let mut bs = BigRef::NIL;
    let mut delta = BigRef::NIL;
    loop {
        bd = ctx.balloc(ctx.k(bd0)).ok_or(DtoaError::NoMemory)?;
        crate::bigint::bcopy(ctx, bd, bd0);
        bb = ctx.balloc(ctx.k(rvb)).ok_or(DtoaError::NoMemory)?;
        crate::bigint::bcopy(ctx, bb, rvb);
        let bbbits = rvbits.wrapping_sub(bb0);
        let bbe = rve.wrapping_add(bb0);
        bs = crate::bigint::i2b(ctx, 1).ok_or(DtoaError::NoMemory)?;
        let mut bb2;
        let bb5;
        let mut bd2;
        let bd5;
        if e >= 0 {
            bb2 = 0;
            bb5 = 0;
            bd2 = e;
            bd5 = e;
        } else {
            bb2 = e.wrapping_neg();
            bb5 = e.wrapping_neg();
            bd2 = 0;
            bd5 = 0;
        }
        if bbe >= 0 {
            bb2 = bb2.wrapping_add(bbe);
        } else {
            bd2 = bd2.wrapping_sub(bbe);
        }
        let mut bs2 = bb2;
        let mut j = nbits.wrapping_add(1).wrapping_sub(bbbits);
        let mut i = bbe.wrapping_add(bbbits).wrapping_sub(nbits);
        if i < emin {
            j = j.wrapping_add(i.wrapping_sub(emin));
        }
        bb2 = bb2.wrapping_add(j);
        bd2 = bd2.wrapping_add(j);
        i = if bb2 < bd2 { bb2 } else { bd2 };
        if i > bs2 {
            i = bs2;
        }
        if i > 0 {
            bb2 = bb2.wrapping_sub(i);
            bd2 = bd2.wrapping_sub(i);
            bs2 = bs2.wrapping_sub(i);
        }
        if bb5 > 0 {
            bs = crate::bigint::pow5mult(ctx, bs, bb5).ok_or(DtoaError::NoMemory)?;
            let bb1 = crate::bigint::mult(ctx, bs, bb).ok_or(DtoaError::NoMemory)?;
            ctx.bfree(bb);
            bb = bb1;
        }
        bb2 = bb2.wrapping_sub(bb0);
        if bb2 > 0 {
            bb = crate::bigint::lshift(ctx, bb, bb2).ok_or(DtoaError::NoMemory)?;
        } else if bb2 < 0 {
            crate::bigint::rshift(ctx, bb, bb2.wrapping_neg());
        }
        if bd5 > 0 {
            bd = crate::bigint::pow5mult(ctx, bd, bd5).ok_or(DtoaError::NoMemory)?;
        }
        if bd2 > 0 {
            bd = crate::bigint::lshift(ctx, bd, bd2).ok_or(DtoaError::NoMemory)?;
        }
        if bs2 > 0 {
            bs = crate::bigint::lshift(ctx, bs, bs2).ok_or(DtoaError::NoMemory)?;
        }
        let mut asub = 1i32;
        let mut inex = STRTOG_INEXHI;
        delta = crate::bigint::diff(ctx, bb, bd).ok_or(DtoaError::NoMemory)?;
        if ctx.wds(delta) <= 1 && ctx.x(delta, 0) == 0 {
            ctx.bfree(delta);
            delta = BigRef::NIL;
            break;
        }
        let dsign = ctx.sign(delta);
        ctx.set_sign(delta, 0);
        let mut finished = 0i32;
        let mut l: i64 = 0;
        i = crate::bigint::cmp(ctx, delta, bs);
        let mut adj = 0.0f64;
        let mut adj0 = 0.0f64;
        let mut do_adj = false;
        let mut undfl = false;
        if rd != 0 && i <= 0 {
            irv = STRTOG_NORMAL;
            finished = dsign ^ (rd & 1);
            if finished != 0 {
                if dsign != 0 {
                    irv |= STRTOG_INEXHI;
                    asub = 0;
                    inex = STRTOG_INEXHI;
                    adj = 1.0;
                    adj0 = 1.0;
                    do_adj = true;
                } else {
                    irv |= STRTOG_INEXLO;
                    let mut adj1 = rve1 == emin;
                    if !adj1 {
                        let mut i2 = 0i32;
                        let mut j2 = nbits;
                        while j2 >= UL_BITS as i32 {
                            if ctx.x(rvb, i2 as u32) & ALL_ON != 0 {
                                adj1 = true;
                                break;
                            }
                            i2 = i2.wrapping_add(1);
                            j2 = j2.wrapping_sub(UL_BITS as i32);
                        }
                        if !adj1 && j2 > 1 {
                            let mut word = ctx.x(rvb, i2 as u32);
                            if crate::bigint::lo0bits(&mut word) < j2 - 1 {
                                adj1 = true;
                            }
                        }
                    }
                    if adj1 {
                        asub = 1;
                        inex = STRTOG_INEXLO;
                        adj = 1.0;
                        adj0 = 1.0;
                        do_adj = true;
                    } else {
                        rve = rve1.wrapping_sub(1);
                        rvb = set_ones(ctx, rvb, nbits).ok_or(DtoaError::NoMemory)?;
                        rvbits = nbits;
                        break;
                    }
                }
            } else {
                irv |= if dsign != 0 {
                    STRTOG_INEXLO
                } else {
                    STRTOG_INEXHI
                };
                break;
            }
        } else if i < 0 {
            irv = if dsign != 0 {
                STRTOG_NORMAL | STRTOG_INEXLO
            } else {
                STRTOG_NORMAL | STRTOG_INEXHI
            };
            if dsign != 0 || bbbits > 1 || denorm != 0 || rve1 == emin {
                break;
            }
            delta = crate::bigint::lshift(ctx, delta, 1).ok_or(DtoaError::NoMemory)?;
            if crate::bigint::cmp(ctx, delta, bs) > 0 {
                irv = STRTOG_NORMAL | STRTOG_INEXLO;
                // drop_down
                if rve1 == emin {
                    irv = STRTOG_NORMAL | STRTOG_INEXHI;
                    break;
                }
                // The donor writes `rve -= nbits`; that is only correct when the power of two
                // sits at bit 0. For a normal value the single set bit is at bit `bb0`, so the
                // exponent of the all-ones significand is `rve + bb0 - nbits` (without the
                // `bb0` term the result is a whole binade too small, e.g. the largest binary128
                // below 2^16369 came back as 2^16257).
                rve = rve.wrapping_add(bb0).wrapping_sub(nbits);
                rvb = set_ones(ctx, rvb, nbits).ok_or(DtoaError::NoMemory)?;
                rvbits = nbits;
                break;
            }
            break;
        } else if i == 0 {
            if dsign != 0 {
                if denorm != 0 && all_on(ctx, rvb, rvbits) {
                    ctx.set_wds(rvb, 1);
                    ctx.set_x(rvb, 0, 1);
                    rve = emin.wrapping_add(nbits).wrapping_sub(1);
                    rvbits = 1;
                    irv = STRTOG_NORMAL | STRTOG_INEXHI;
                    denorm = 0;
                    break;
                }
                irv = STRTOG_NORMAL | STRTOG_INEXLO;
            } else if bbbits == 1 {
                irv = STRTOG_NORMAL;
                // drop_down
                if rve1 == emin {
                    irv = STRTOG_NORMAL | STRTOG_INEXHI;
                    if ctx.wds(rvb) == 1 && ctx.x(rvb, 0) == 1 {
                        sudden_underflow = 1;
                    }
                    break;
                }
                // Same correction as the `i < 0` drop_down: see the comment above.
                rve = rve.wrapping_add(bb0).wrapping_sub(nbits);
                rvb = set_ones(ctx, rvb, nbits).ok_or(DtoaError::NoMemory)?;
                rvbits = nbits;
                break;
            } else {
                irv = STRTOG_NORMAL | STRTOG_INEXHI;
            }
            if (bbbits < nbits && denorm == 0) || (ctx.x(rvb, 0) & 1) == 0 {
                break;
            }
            if dsign != 0 {
                rvb = increment(ctx, rvb).ok_or(DtoaError::NoMemory)?;
                let j = KMASK & UL_BITS.wrapping_sub((rvbits as u32) & KMASK);
                if crate::bigint::hi0bits(ctx.x(rvb, (ctx.wds(rvb) - 1) as u32)) != j as i32 {
                    rvbits = rvbits.wrapping_add(1);
                }
                irv = STRTOG_NORMAL | STRTOG_INEXHI;
            } else {
                if bbbits == 1 {
                    asub = 1;
                    inex = STRTOG_INEXLO;
                    undfl = true;
                } else {
                    decrement(ctx, rvb);
                    irv = STRTOG_NORMAL | STRTOG_INEXLO;
                }
            }
            if !undfl {
                break;
            }
        } else {
            adj = crate::bigint::ratio(ctx, delta, bs);
            if adj <= 2.0 {
                asub = 1;
                inex = STRTOG_INEXLO;
                if dsign != 0 {
                    asub = 0;
                    inex = STRTOG_INEXHI;
                } else if denorm != 0 && bbbits <= 1 {
                    undfl = true;
                }
                adj0 = 1.0;
                adj = 1.0;
                do_adj = !undfl;
            } else {
                adj0 = adj * 0.5;
                adj = adj0;
                if dsign != 0 {
                    asub = 0;
                    inex = STRTOG_INEXLO;
                }
                if adj < 2147483647.0 {
                    l = adj0 as i64;
                    adj0 -= l as f64;
                    match rd {
                        0 => {
                            if adj0 >= 0.5 {
                                l = l.wrapping_add(1);
                                inex = STRTOG_INEXACT.wrapping_sub(inex);
                            }
                        }
                        1 => {
                            if asub != 0 && adj0 > 0.0 {
                                l = l.wrapping_add(1);
                                inex = STRTOG_INEXACT.wrapping_sub(inex);
                            }
                        }
                        2 => {
                            if asub == 0 && adj0 > 0.0 {
                                l = l.wrapping_add(1);
                                inex = STRTOG_INEXACT.wrapping_sub(inex);
                            }
                        }
                        _ => {}
                    }
                    adj = l as f64;
                }
                do_adj = true;
            }
        }
        if undfl {
            ctx.set_wds(rvb, 0);
            rve = emin;
            irv = STRTOG_UNDERFLOW | STRTOG_INEXLO;
            break;
        }
        if do_adj {
            let y2 = rve.wrapping_add(rvbits);
            if denorm == 0 && rvbits < nbits {
                let j = nbits.wrapping_sub(rvbits);
                rvb = crate::bigint::lshift(ctx, rvb, j).ok_or(DtoaError::NoMemory)?;
                rve = rve.wrapping_sub(j);
                rvbits = nbits;
            }
            let mut abe = 0i32;
            let mut abits = 0i32;
            let mut ab =
                crate::bigint::d2b(ctx, adj, &mut abe, &mut abits).ok_or(DtoaError::NoMemory)?;
            if abe < 0 {
                crate::bigint::rshift(ctx, ab, abe.wrapping_neg());
            } else if abe > 0 {
                ab = crate::bigint::lshift(ctx, ab, abe).ok_or(DtoaError::NoMemory)?;
            }
            let rvb0 = rvb;
            if asub != 0 {
                rvb = crate::bigint::diff(ctx, rvb, ab).ok_or(DtoaError::NoMemory)?;
                let k2 = ctx.wds(rvb0).wrapping_sub(1);
                if denorm == 0
                    && (ctx.wds(rvb) <= k2
                        || crate::bigint::hi0bits(ctx.x(rvb, k2 as u32))
                            > crate::bigint::hi0bits(ctx.x(rvb0, k2 as u32)))
                {
                    if rve1 == emin {
                        rvbits = rvbits.wrapping_sub(1);
                        denorm = 1;
                    } else {
                        rvb = crate::bigint::lshift(ctx, rvb, 1).ok_or(DtoaError::NoMemory)?;
                        rve = rve.wrapping_sub(1);
                        rve1 = rve1.wrapping_sub(1);
                        l = 0;
                        finished = 0;
                    }
                }
            } else {
                rvb = crate::bigint::sum(ctx, rvb, ab).ok_or(DtoaError::NoMemory)?;
                let k2 = ctx.wds(rvb).wrapping_sub(1);
                if k2 >= ctx.wds(rvb0)
                    || crate::bigint::hi0bits(ctx.x(rvb, k2 as u32))
                        < crate::bigint::hi0bits(ctx.x(rvb0, k2 as u32))
                {
                    if denorm != 0 {
                        rvbits = rvbits.wrapping_add(1);
                        if rvbits == nbits {
                            denorm = 0;
                        }
                    } else {
                        crate::bigint::rshift(ctx, rvb, 1);
                        rve = rve.wrapping_add(1);
                        rve1 = rve1.wrapping_add(1);
                        l = 0;
                    }
                }
            }
            ctx.bfree(ab);
            ctx.bfree(rvb0);
            if finished != 0 {
                break;
            }
            let z2 = rve.wrapping_add(rvbits);
            if y2 == z2 && l != 0 {
                let tol = adj * 5e-16;
                adj0 -= 0.5;
                if adj0 < -tol {
                    if adj0 > tol {
                        irv |= inex;
                        break;
                    }
                } else if adj0 > tol && adj0 < 1.0 - tol {
                    irv |= inex;
                    break;
                }
            }
            bb0 = if denorm != 0 {
                0
            } else {
                crate::bigint::trailz(ctx, rvb)
            };
        }
        ctx.bfree(bb);
        ctx.bfree(bd);
        ctx.bfree(bs);
        ctx.bfree(delta);
        bb = BigRef::NIL;
        bd = BigRef::NIL;
        bs = BigRef::NIL;
        delta = BigRef::NIL;
    }
    if denorm == 0 {
        let j = nbits.wrapping_sub(rvbits);
        if j != 0 {
            if j > 0 {
                rvb = crate::bigint::lshift(ctx, rvb, j).ok_or(DtoaError::NoMemory)?;
            } else {
                crate::bigint::rshift(ctx, rvb, j.wrapping_neg());
            }
            rve = rve.wrapping_sub(j);
        }
    }
    *expt = rve;
    ctx.bfree(bb);
    ctx.bfree(bd);
    ctx.bfree(bs);
    ctx.bfree(bd0);
    ctx.bfree(delta);
    if rve > fpi.emax {
        let round_up = match fpi.rounding & 3 {
            FPI_ROUND_NEAR => true,
            FPI_ROUND_UP => sign == 0,
            FPI_ROUND_DOWN => sign != 0,
            _ => false,
        };
        if round_up {
            ctx.bfree(rvb);
            rvb = BigRef::NIL;
            irv = STRTOG_INFINITE | STRTOG_OVERFLOW | STRTOG_INEXHI;
            *expt = fpi.emax.wrapping_add(1);
        } else {
            ctx.bfree(rvb);
            rvb = BigRef::NIL;
            irv = STRTOG_NORMAL | STRTOG_INEXLO;
            *expt = fpi.emax;
            set_max_bits(bits, nbits);
        }
    }
    ret(
        s,
        sign,
        irv,
        denorm,
        sudden_underflow,
        rvb,
        bits,
        nbits,
        ctx,
    )
}

/// Write the all-ones significand for "round to largest representable magnitude".
fn set_max_bits(bits: &mut [u32], nbits: i32) {
    let be = ((nbits.wrapping_add(31)) as u32 >> 5) as usize;
    let mut i = 0usize;
    while i < be {
        if let Some(slot) = bits.get_mut(i) {
            *slot = ALL_ON;
        }
        i = i.wrapping_add(1);
    }
    let j = nbits & 0x1f;
    if j != 0 {
        let idx = be.wrapping_sub(1);
        if let Some(slot) = bits.get_mut(idx) {
            *slot >>= 32u32.wrapping_sub(j as u32);
        }
    }
}

/// Shared `ret` epilogue.
#[allow(clippy::too_many_arguments)]
fn ret(
    s: usize,
    sign: i32,
    mut irv: i32,
    denorm: i32,
    sudden_underflow: i32,
    rvb: BigRef,
    bits: &mut [u32],
    nbits: i32,
    ctx: &mut DtoaCtx,
) -> Result<(usize, i32), DtoaError> {
    if denorm != 0 {
        if sudden_underflow != 0 {
            ctx.set_wds(rvb, 0);
            irv = STRTOG_UNDERFLOW | STRTOG_INEXLO;
        } else {
            irv = (irv & !STRTOG_RETMASK)
                | if ctx.wds(rvb) > 0 {
                    STRTOG_DENORMAL
                } else {
                    STRTOG_ZERO
                };
            if irv & STRTOG_INEXACT != 0 {
                irv |= STRTOG_UNDERFLOW;
            }
        }
    }
    if sign != 0 {
        irv |= STRTOG_NEG;
    }
    if !rvb.is_nil() {
        crate::bigint::copybits(bits, nbits, rvb, ctx);
        ctx.bfree(rvb);
    }
    Ok((s, irv))
}

#[cfg(test)]
#[path = "strtodg_ut.rs"]
mod tests;
