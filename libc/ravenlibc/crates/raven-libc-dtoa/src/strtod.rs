// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998-2001 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/strtod.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: strtod.c,v 1.20 2024/01/23 15:31:58 christos Exp $)
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
//! `strtod.c` of NetBSD gdtoa: the correctly rounded binary64 parser.
//!
//! The dedicated double path (A-num-1): a fast `double` approximation followed by the big-integer
//! correction. `localeconv_l(loc)->decimal_point` is the [`NumericLocale`] parameter, the
//! `errno = ERANGE` sites are range flags, and every `Honor_FLT_ROUNDS` guard is the [`Rounding`]
//! parameter.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::config::{
    BIAS, BIG0, BIG1, BIGTENS, BNDRY_MASK, DBL_DIG, DBL_MAX_10_EXP, DBL_MAX_EXP, EMIN, EXP_1,
    EXP_MASK, EXP_MSK1, EXP_SHIFT, FRAC_MASK, LSB, N_BIGTENS, P, STRTO_TINYTENS, TEN_PMAX, TENS,
    TINY1, at, from_words, set_word0, set_word1, w0_add, w0_and, w0_sub, w1_and, word0, word1,
};
use crate::ctx::{BigRef, DtoaCtx};
use crate::types::*;
use crate::{DtoaError, NumericLocale, Rounding};

/// `sulp`: the scaled ulp used by the `Avoid_Underflow` half-way adjustments.
fn sulp(x: f64, scale: i32) -> f64 {
    let mut rv = crate::ulp::ulp(x);
    if scale == 0 {
        return rv;
    }
    let i = (2i32.wrapping_mul(P))
        .wrapping_add(1)
        .wrapping_sub(((word0(x) & EXP_MASK) >> EXP_SHIFT) as i32);
    if i <= 0 {
        return rv;
    }
    let u = from_words(EXP_1.wrapping_add((i as u32) << EXP_SHIFT), 0);
    rv *= u;
    rv
}

/// The donor's `ovfl` value after the sign-adjusted `Rounding`.
fn overflow_value(rounding: Rounding, sign: i32) -> f64 {
    let _ = sign;
    match rounding {
        Rounding::TowardZero | Rounding::Downward => from_words(BIG0, BIG1),
        _ => from_words(EXP_MASK, 0),
    }
}

/// `strtod`: parse a decimal/hex/inf/nan string into a `double`.
#[allow(unused_assignments, clippy::collapsible_if, clippy::collapsible_match)] // donor initialisations and nested special-case tests
pub(crate) fn strtod(
    ctx: &mut DtoaCtx,
    input: &[u8],
    loc: &NumericLocale,
    rounding: Rounding,
) -> Result<(f64, usize, i32), DtoaError> {
    let input_start = 0usize;
    let mut s = 0usize;
    let mut sign = 0i32;
    let mut rv: f64 = 0.0;
    let mut range = 0i32;
    let dplen = loc.decimal_point.len();
    let original_rounding = rounding;
    let mut rounding = rounding;

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
                    return Ok((0.0, 0, 0));
                }
            }
            b'+' => {
                s = s.wrapping_add(1);
                if at(input, s) != 0 {
                    skip = false;
                } else {
                    return Ok((0.0, 0, 0));
                }
            }
            0 => return Ok((0.0, 0, 0)),
            b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | b' ' => {
                s = s.wrapping_add(1);
            }
            _ => skip = false,
        }
    }

    let mut nz0 = 0i32;
    let mut nz = 0i32;
    if at(input, s) == b'0' {
        match at(input, s.wrapping_add(1)) {
            b'x' | b'X' => {
                let fpi = Fpi::double().with_rounding(rounding.as_fpi());
                let mut expt = 0i32;
                let mut bb = BigRef::NIL;
                let mut bits = [0u32; 2];
                let k =
                    crate::gethex::gethex(ctx, input, &mut s, &fpi, &mut expt, &mut bb, sign, loc)?;
                match k & STRTOG_RETMASK {
                    STRTOG_NONUMBER => {
                        s = input_start;
                        sign = 0;
                    }
                    STRTOG_ZERO => {}
                    _ => {
                        if !bb.is_nil() {
                            crate::bigint::copybits(&mut bits, fpi.nbits, bb, ctx);
                            ctx.bfree(bb);
                        }
                        rv = crate::strtord::ultod(&bits, expt, k);
                    }
                }
                let flags = range_from_strtog(k);
                return Ok((if sign != 0 { -rv } else { rv }, s, flags));
            }
            _ => {}
        }
        nz0 = 1;
        s = s.wrapping_add(1);
        while at(input, s) == b'0' {
            s = s.wrapping_add(1);
        }
        if at(input, s) == 0 {
            return Ok((if sign != 0 { -0.0 } else { 0.0 }, s, 0));
        }
    }
    let mut s0 = s;
    let mut y: u32 = 0;
    let mut z: u32 = 0;
    let mut nd = 0i32;
    let mut nf = 0i32;
    let mut c = at(input, s);
    while c.is_ascii_digit() {
        if nd < 9 {
            y = y.wrapping_mul(10).wrapping_add(u32::from(c - b'0'));
        } else if nd < DBL_DIG + 2 {
            z = z.wrapping_mul(10).wrapping_add(u32::from(c - b'0'));
        }
        nd = nd.wrapping_add(1);
        s = s.wrapping_add(1);
        c = at(input, s);
    }
    let nd0 = nd;
    let dot = loc.decimal_point.first().copied().unwrap_or(b'.');
    let mut decpt = false;
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
            decpt = true;
            let mut have_dig = nd != 0;
            if nd == 0 {
                while c == b'0' {
                    nz = nz.wrapping_add(1);
                    s = s.wrapping_add(1);
                    c = at(input, s);
                }
                if c > b'0' && c <= b'9' {
                    s0 = s;
                    // The donor folds the skipped zeros into `nf` before jumping to
                    // `have_dig`; `nz` must start at zero for the first significant digit.
                    nf = nf.wrapping_add(nz);
                    nz = 0;
                    have_dig = true;
                }
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
                            if nd < 9 {
                                y = y.wrapping_mul(10);
                            } else if nd <= DBL_DIG + 2 {
                                z = z.wrapping_mul(10);
                            }
                            nd = nd.wrapping_add(1);
                            i = i.wrapping_add(1);
                        }
                        if nd < 9 {
                            y = y.wrapping_mul(10).wrapping_add(digit);
                        } else if nd <= DBL_DIG + 2 {
                            z = z.wrapping_mul(10).wrapping_add(digit);
                        }
                        nd = nd.wrapping_add(1);
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
            return Ok((0.0, 0, 0));
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
            if !decpt {
                match c {
                    b'i' | b'I' => {
                        if crate::bigint::match_prefix(input, &mut s, b"nf") {
                            s = s.wrapping_sub(1);
                            if !crate::bigint::match_prefix(input, &mut s, b"inity") {
                                s = s.wrapping_add(1);
                            }
                            rv = from_words(0x7ff0_0000, 0);
                            infnan = true;
                        }
                    }
                    b'n' | b'N' => {
                        if crate::bigint::match_prefix(input, &mut s, b"an") {
                            let fpinan = Fpi {
                                nbits: 52,
                                emin: 1 - 1023 - 53 + 1,
                                emax: 2046 - 1023 - 53 + 1,
                                rounding: FPI_ROUND_NEAR,
                                sudden_underflow: 0,
                            };
                            let mut bits = [0u32; 2];
                            let nanbits = if at(input, s) == b'(' {
                                crate::hexnan::hexnan(input, &mut s, &fpinan, &mut bits)
                            } else {
                                STRTOG_NAN
                            };
                            if nanbits == STRTOG_NANBITS {
                                rv = from_words(0x7ff0_0000 | bits[1], bits[0]);
                            } else {
                                rv = from_words(crate::config::D_QNAN1, crate::config::D_QNAN0);
                            }
                            infnan = true;
                        }
                    }
                    _ => {}
                }
            }
            if !infnan {
                return Ok((0.0, 0, 0));
            }
        }
        return Ok((if sign != 0 { -rv } else { rv }, s, range));
    }
    let mut e1 = e.wrapping_sub(nf);
    e = e1;
    let k = if nd < DBL_DIG + 2 { nd } else { DBL_DIG + 2 };
    rv = f64::from(y);
    if k > 9 {
        rv = TENS[(k - 9) as usize] * rv + f64::from(z);
    }
    if rounding == Rounding::Nearest && nd <= DBL_DIG {
        if e == 0 {
            return Ok((if sign != 0 { -rv } else { rv }, s, range));
        }
        if e > 0 {
            if e <= TEN_PMAX {
                if sign != 0 {
                    rv = -rv;
                    sign = 0;
                }
                rv *= TENS[e as usize];
                return Ok((rv, s, range));
            }
            let i = DBL_DIG.wrapping_sub(nd);
            if e <= TEN_PMAX.wrapping_add(i) {
                if sign != 0 {
                    rv = -rv;
                    sign = 0;
                }
                let e2 = e.wrapping_sub(i);
                e1 = e1.wrapping_sub(i);
                rv *= TENS[i as usize];
                rv *= TENS[e2 as usize];
                return Ok((rv, s, range));
            }
        } else if e >= TEN_PMAX.wrapping_neg() {
            if sign != 0 {
                rv = -rv;
                sign = 0;
            }
            rv /= TENS[e.wrapping_neg() as usize];
            return Ok((rv, s, range));
        }
    }
    e1 = e1.wrapping_add(nd.wrapping_sub(k));

    let mut scale = 0i32;
    if matches!(rounding, Rounding::Upward | Rounding::Downward) {
        if sign != 0 {
            rounding = if rounding == Rounding::Upward {
                Rounding::TowardZero
            } else {
                Rounding::Upward
            };
        } else if rounding != Rounding::Upward {
            rounding = Rounding::TowardZero;
        }
    }

    if e1 > 0 {
        let i = e1 & 15;
        if i != 0 {
            rv *= TENS[i as usize];
        }
        let mut e1b = e1 & !15;
        if e1b != 0 {
            if e1b > DBL_MAX_10_EXP {
                rv = overflow_value(rounding, sign);
                range |= STRTOG_OVERFLOW;
                return Ok((if sign != 0 { -rv } else { rv }, s, range));
            }
            e1b = (e1b as u32 >> 4) as i32;
            let mut j = 0usize;
            while e1b > 1 {
                if e1b & 1 != 0 {
                    rv *= BIGTENS[j];
                }
                j = j.wrapping_add(1);
                e1b = (e1b as u32 >> 1) as i32;
            }
            w0_sub(&mut rv, (P as u32).wrapping_mul(EXP_MSK1));
            rv *= BIGTENS[j];
            let zz = word0(rv) & EXP_MASK;
            if zz > (EXP_MSK1.wrapping_mul((DBL_MAX_EXP + BIAS - P) as u32)) {
                rv = overflow_value(rounding, sign);
                range |= STRTOG_OVERFLOW;
                return Ok((if sign != 0 { -rv } else { rv }, s, range));
            }
            if zz > EXP_MSK1.wrapping_mul((DBL_MAX_EXP + BIAS - 1 - P) as u32) {
                rv = from_words(BIG0, BIG1);
            } else {
                w0_add(&mut rv, (P as u32).wrapping_mul(EXP_MSK1));
            }
        }
    } else if e1 < 0 {
        e1 = e1.wrapping_neg();
        let i = e1 & 15;
        if i != 0 {
            rv /= TENS[i as usize];
        }
        e1 = (e1 as u32 >> 4) as i32;
        if e1 != 0 {
            if e1 >= (1i32 << N_BIGTENS) {
                rv = 0.0;
                if rounding == Rounding::Upward {
                    set_word1(&mut rv, 1);
                }
                range |= STRTOG_UNDERFLOW;
                return Ok((if sign != 0 { -rv } else { rv }, s, range));
            }
            if e1 & crate::config::SCALE_BIT != 0 {
                scale = 2i32.wrapping_mul(P);
            }
            let mut j = 0usize;
            let mut e1b = e1;
            while e1b > 0 {
                if e1b & 1 != 0 {
                    rv *= STRTO_TINYTENS[j];
                }
                j = j.wrapping_add(1);
                e1b = (e1b as u32 >> 1) as i32;
            }
            if scale != 0 {
                let expf = ((word0(rv) & EXP_MASK) >> EXP_SHIFT) as i32;
                let jj = 2i32.wrapping_mul(P).wrapping_add(1).wrapping_sub(expf);
                if jj > 0 {
                    if jj >= 32 {
                        set_word1(&mut rv, 0);
                        if jj >= 53 {
                            set_word0(&mut rv, (P as u32 + 2).wrapping_mul(EXP_MSK1));
                        } else {
                            w0_and(&mut rv, 0xffff_ffffu32 << (jj - 32));
                        }
                    } else {
                        w1_and(&mut rv, 0xffff_ffffu32 << jj);
                    }
                }
            }
        }
    }

    let mut bb = BigRef::NIL;
    let mut bd = BigRef::NIL;
    let mut bs = BigRef::NIL;
    let mut delta = BigRef::NIL;
    let s2b_dplen = if nd0 == 0 { 0 } else { dplen };
    let bd0 =
        crate::bigint::s2b(ctx, &input[s0..], nd0, nd, y, s2b_dplen).ok_or(DtoaError::NoMemory)?;
    'adjust: loop {
        bd = ctx.balloc(ctx.k(bd0)).ok_or(DtoaError::NoMemory)?;
        crate::bigint::bcopy(ctx, bd, bd0);
        let mut bbe = 0i32;
        let mut bbbits = 0i32;
        bb = crate::bigint::d2b(ctx, rv, &mut bbe, &mut bbbits).ok_or(DtoaError::NoMemory)?;
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
        if rounding != Rounding::Nearest {
            bs2 = bs2.wrapping_add(1);
        }
        let mut lsb = LSB;
        let mut lsb1 = 0u32;
        let j = bbe.wrapping_sub(scale);
        let i = j.wrapping_add(bbbits).wrapping_sub(1);
        let mut j = P.wrapping_add(1).wrapping_sub(bbbits);
        if i < EMIN {
            let i2 = EMIN.wrapping_sub(i);
            j = j.wrapping_sub(i2);
            if i2 < 32 {
                lsb = lsb.wrapping_shl(i2 as u32);
            } else {
                lsb1 = lsb.wrapping_shl((i2 - 32) as u32);
            }
        }
        bb2 = bb2.wrapping_add(j);
        bd2 = bd2.wrapping_add(j);
        bd2 = bd2.wrapping_add(scale);
        let mut i = if bb2 < bd2 { bb2 } else { bd2 };
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
        if bb2 > 0 {
            bb = crate::bigint::lshift(ctx, bb, bb2).ok_or(DtoaError::NoMemory)?;
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
        delta = crate::bigint::diff(ctx, bb, bd).ok_or(DtoaError::NoMemory)?;
        let mut dsign = ctx.sign(delta);
        ctx.set_sign(delta, 0);
        i = crate::bigint::cmp(ctx, delta, bs);

        if rounding != Rounding::Nearest {
            if i < 0 {
                if ctx.wds(delta) <= 1 && ctx.x(delta, 0) == 0 {
                    ctx.bfree(delta);
                    delta = BigRef::NIL;
                    break;
                }
                let mut apply = false;
                let mut adj = 0.0f64;
                if rounding != Rounding::TowardZero {
                    if dsign != 0 {
                        adj = 1.0;
                        apply = true;
                    }
                } else if dsign == 0 {
                    adj = -1.0;
                    if word1(rv) == 0 && word0(rv) & FRAC_MASK == 0 {
                        let y2 = word0(rv) & EXP_MASK;
                        if scale == 0 || y2 > (2u32.wrapping_mul(P as u32)).wrapping_mul(EXP_MSK1) {
                            delta =
                                crate::bigint::lshift(ctx, delta, 1).ok_or(DtoaError::NoMemory)?;
                            if crate::bigint::cmp(ctx, delta, bs) <= 0 {
                                adj = -0.5;
                            }
                        }
                    }
                    apply = true;
                }
                if apply {
                    if scale != 0 {
                        let y2 = word0(rv) & EXP_MASK;
                        if y2 <= (2u32.wrapping_mul(P as u32)).wrapping_mul(EXP_MSK1) {
                            w0_add(
                                &mut adj,
                                (2u32.wrapping_mul(P as u32))
                                    .wrapping_add(1)
                                    .wrapping_mul(EXP_MSK1)
                                    .wrapping_sub(y2),
                            );
                        }
                    }
                    rv += adj * crate::ulp::ulp(rv);
                }
                ctx.bfree(delta);
                delta = BigRef::NIL;
                break;
            }
            let mut adj = crate::bigint::ratio(ctx, delta, bs);
            if adj < 1.0 {
                adj = 1.0;
            }
            if adj <= 0x7fff_fffeu32 as f64 {
                let mut y2 = adj as u32;
                if f64::from(y2) != adj {
                    if ((rounding as u32 >> 1) ^ (dsign as u32)) == 0 {
                        y2 = y2.wrapping_add(1);
                    }
                    adj = f64::from(y2);
                }
            }
            if scale != 0 {
                let y2 = word0(rv) & EXP_MASK;
                if y2 <= (2u32.wrapping_mul(P as u32)).wrapping_mul(EXP_MSK1) {
                    w0_add(
                        &mut adj,
                        (2u32.wrapping_mul(P as u32))
                            .wrapping_add(1)
                            .wrapping_mul(EXP_MSK1)
                            .wrapping_sub(y2),
                    );
                }
            }
            adj *= crate::ulp::ulp(rv);
            if dsign != 0 {
                if word0(rv) == BIG0 && word1(rv) == BIG1 {
                    rv = overflow_value(rounding, sign);
                    range |= STRTOG_OVERFLOW;
                    break 'adjust;
                }
                rv += adj;
            } else {
                rv -= adj;
            }
            ctx.bfree(bb);
            ctx.bfree(bd);
            ctx.bfree(bs);
            ctx.bfree(delta);
            bb = BigRef::NIL;
            bd = BigRef::NIL;
            bs = BigRef::NIL;
            delta = BigRef::NIL;
            continue;
        }

        if i < 0 {
            if dsign != 0
                || word1(rv) != 0
                || word0(rv) & BNDRY_MASK != 0
                || (word0(rv) & EXP_MASK)
                    <= (2u32.wrapping_mul(P as u32))
                        .wrapping_add(1)
                        .wrapping_mul(EXP_MSK1)
            {
                ctx.bfree(delta);
                delta = BigRef::NIL;
                break;
            }
            if ctx.wds(delta) <= 1 && ctx.x(delta, 0) == 0 {
                ctx.bfree(delta);
                delta = BigRef::NIL;
                break;
            }
            delta = crate::bigint::lshift(ctx, delta, 1).ok_or(DtoaError::NoMemory)?;
            if crate::bigint::cmp(ctx, delta, bs) > 0 {
                // drop_down
                if scale != 0 {
                    let l = word0(rv) & EXP_MASK;
                    if l <= (2u32.wrapping_mul(P as u32))
                        .wrapping_add(1)
                        .wrapping_mul(EXP_MSK1)
                    {
                        if l > (P as u32 + 2).wrapping_mul(EXP_MSK1) {
                            ctx.bfree(delta);
                            delta = BigRef::NIL;
                            break;
                        }
                        rv = 0.0;
                        if rounding == Rounding::Upward {
                            set_word1(&mut rv, 1);
                        }
                        range |= STRTOG_UNDERFLOW;
                        break 'adjust;
                    }
                }
                let l = (word0(rv) & EXP_MASK).wrapping_sub(EXP_MSK1);
                set_word0(&mut rv, l | BNDRY_MASK1_CONST);
                set_word1(&mut rv, 0xffff_ffff);
                ctx.bfree(delta);
                delta = BigRef::NIL;
                break;
            }
            ctx.bfree(delta);
            delta = BigRef::NIL;
            break;
        }
        if i == 0 {
            if dsign != 0 {
                if (word0(rv) & BNDRY_MASK) == BNDRY_MASK
                    && word1(rv)
                        == if scale != 0
                            && (word0(rv) & EXP_MASK)
                                <= (2u32.wrapping_mul(P as u32)).wrapping_mul(EXP_MSK1)
                        {
                            let y2 = word0(rv) & EXP_MASK;
                            let shift = 2u32
                                .wrapping_mul(P as u32)
                                .wrapping_add(1)
                                .wrapping_sub(y2 >> EXP_SHIFT);
                            0xffff_ffffu32.wrapping_shl(shift)
                        } else {
                            0xffff_ffff
                        }
                {
                    if word0(rv) == BIG0 && word1(rv) == BIG1 {
                        rv = overflow_value(rounding, sign);
                        range |= STRTOG_OVERFLOW;
                        break 'adjust;
                    }
                    let value = (word0(rv) & EXP_MASK).wrapping_add(EXP_MSK1);
                    set_word0(&mut rv, value);
                    set_word1(&mut rv, 0);
                    dsign = 0;
                    ctx.bfree(delta);
                    delta = BigRef::NIL;
                    break;
                }
            } else if word0(rv) & BNDRY_MASK == 0 && word1(rv) == 0 {
                // drop_down
                if scale != 0 {
                    let l = word0(rv) & EXP_MASK;
                    if l <= (2u32.wrapping_mul(P as u32))
                        .wrapping_add(1)
                        .wrapping_mul(EXP_MSK1)
                    {
                        if l > (P as u32 + 2).wrapping_mul(EXP_MSK1) {
                            ctx.bfree(delta);
                            delta = BigRef::NIL;
                            break;
                        }
                        rv = 0.0;
                        if rounding == Rounding::Upward {
                            set_word1(&mut rv, 1);
                        }
                        range |= STRTOG_UNDERFLOW;
                        break 'adjust;
                    }
                }
                let l = (word0(rv) & EXP_MASK).wrapping_sub(EXP_MSK1);
                set_word0(&mut rv, l | BNDRY_MASK1_CONST);
                set_word1(&mut rv, 0xffff_ffff);
                ctx.bfree(delta);
                delta = BigRef::NIL;
                break;
            }
            let half = if lsb1 != 0 {
                word0(rv) & lsb1 != 0
            } else {
                word1(rv) & lsb != 0
            };
            if !half {
                ctx.bfree(delta);
                delta = BigRef::NIL;
                break;
            }
            if dsign != 0 {
                rv += sulp(rv, scale);
            } else {
                rv -= sulp(rv, scale);
                if rv == 0.0 {
                    rv = 0.0;
                    if rounding == Rounding::Upward {
                        set_word1(&mut rv, 1);
                    }
                    range |= STRTOG_UNDERFLOW;
                    break 'adjust;
                }
            }
            dsign = 1i32.wrapping_sub(dsign);
            ctx.bfree(delta);
            delta = BigRef::NIL;
            break;
        }

        let mut aadj = crate::bigint::ratio(ctx, delta, bs);
        let mut aadj1 = 0.0f64;
        let mut adj = 0.0f64;
        if aadj <= 2.0 {
            if dsign != 0 {
                aadj = 1.0;
                aadj1 = 1.0;
            } else if word1(rv) != 0 || word0(rv) & BNDRY_MASK != 0 {
                if word1(rv) == TINY1 && word0(rv) == 0 {
                    rv = 0.0;
                    if rounding == Rounding::Upward {
                        set_word1(&mut rv, 1);
                    }
                    range |= STRTOG_UNDERFLOW;
                    break 'adjust;
                }
                aadj = 1.0;
                aadj1 = -1.0;
            } else {
                if aadj < 2.0 / crate::config::FLT_RADIX {
                    aadj = 1.0 / crate::config::FLT_RADIX;
                } else {
                    aadj *= 0.5;
                }
                aadj1 = -aadj;
            }
        } else {
            aadj *= 0.5;
            aadj1 = if dsign != 0 { aadj } else { -aadj };
            match rounding {
                Rounding::Upward => aadj1 -= 0.5,
                Rounding::TowardZero | Rounding::Downward => aadj1 += 0.5,
                _ => {}
            }
        }
        let y2 = word0(rv) & EXP_MASK;
        if y2 == EXP_MSK1.wrapping_mul((DBL_MAX_EXP + BIAS - 1) as u32) {
            let rv0 = rv;
            w0_sub(&mut rv, (P as u32).wrapping_mul(EXP_MSK1));
            adj = aadj1 * crate::ulp::ulp(rv);
            rv += adj;
            if word0(rv) & EXP_MASK >= EXP_MSK1.wrapping_mul((DBL_MAX_EXP + BIAS - P) as u32) {
                if word0(rv0) == BIG0 && word1(rv0) == BIG1 {
                    rv = overflow_value(rounding, sign);
                    range |= STRTOG_OVERFLOW;
                    break 'adjust;
                }
                rv = from_words(BIG0, BIG1);
                ctx.bfree(bb);
                ctx.bfree(bd);
                ctx.bfree(bs);
                ctx.bfree(delta);
                bb = BigRef::NIL;
                bd = BigRef::NIL;
                bs = BigRef::NIL;
                delta = BigRef::NIL;
                continue;
            }
            w0_add(&mut rv, (P as u32).wrapping_mul(EXP_MSK1));
        } else {
            if scale != 0 && y2 <= (2u32.wrapping_mul(P as u32)).wrapping_mul(EXP_MSK1) {
                if aadj <= 0x7fff_ffffu32 as f64 {
                    let mut zz = aadj as u32;
                    if zz == 0 {
                        zz = 1;
                    }
                    aadj = f64::from(zz);
                    aadj1 = if dsign != 0 { aadj } else { -aadj };
                }
                w0_add(
                    &mut aadj1,
                    (2u32.wrapping_mul(P as u32))
                        .wrapping_add(1)
                        .wrapping_mul(EXP_MSK1)
                        .wrapping_sub(y2),
                );
            }
            adj = aadj1 * crate::ulp::ulp(rv);
            rv += adj;
        }
        let z2 = word0(rv) & EXP_MASK;
        if scale == 0 && y2 == z2 {
            let l = aadj as i64;
            aadj -= l as f64;
            if dsign != 0 || word1(rv) != 0 || word0(rv) & BNDRY_MASK != 0 {
                if !(0.4999999..=0.5000001).contains(&aadj) {
                    ctx.bfree(delta);
                    delta = BigRef::NIL;
                    break;
                }
            } else if aadj < 0.4999999 / crate::config::FLT_RADIX {
                ctx.bfree(delta);
                delta = BigRef::NIL;
                break;
            }
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
    ctx.bfree(bb);
    ctx.bfree(bd);
    ctx.bfree(bs);
    ctx.bfree(bd0);
    ctx.bfree(delta);
    if scale != 0 {
        let rv0 = from_words(
            EXP_1.wrapping_sub((2u32.wrapping_mul(P as u32)).wrapping_mul(EXP_MSK1)),
            0,
        );
        rv *= rv0;
        if word0(rv) & EXP_MASK == 0 {
            range |= STRTOG_UNDERFLOW;
            // The donor performs its final directed adjustment with the host FPU in the caller's
            // mode; an exact zero from opposite-signed operands is -0.0 under downward rounding
            // (IEEE 754 sign rule), while the Rust port is mode-independent. Reproduce the sign
            // the donor produces for the scale-path underflow.
            if rv == 0.0 && original_rounding == Rounding::Downward && sign == 0 {
                rv = -0.0;
            }
        }
    }
    Ok((if sign != 0 { -rv } else { rv }, s, range))
}

/// The donor's `Bndry_mask1`.
const BNDRY_MASK1_CONST: u32 = 0x000f_ffff;

/// Map `STRTOG_*` flags to the public range set without exposing the donor sentinels.
pub(crate) fn range_from_strtog(flags: i32) -> i32 {
    let mut range = 0i32;
    if flags & STRTOG_OVERFLOW != 0 {
        range |= STRTOG_OVERFLOW;
    }
    if flags & STRTOG_UNDERFLOW != 0 {
        range |= STRTOG_UNDERFLOW;
    }
    range
}

#[cfg(test)]
#[path = "strtod_ut.rs"]
mod tests;
