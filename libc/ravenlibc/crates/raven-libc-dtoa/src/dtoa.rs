// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998, 1999 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/dtoa.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: dtoa.c,v 1.12 2024/01/20 14:52:46 christos Exp $)
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
//! `dtoa.c` of NetBSD gdtoa: binary64 to decimal digits.
//!
//! Modes 0-5 of `dtoa(d, mode, ndigits, &decpt, &sign, &rve)`; the result is written into the
//! caller's buffer (no NUL, no trailing zeros). Inf/NaN become [`crate::Kind`]; the donor's
//! `decpt = 9999` sentinel is not exposed. Every `Honor_FLT_ROUNDS` guard is the [`Rounding`]
//! parameter, and the free-list/pool pointers are the [`DtoaCtx`] arena.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::config::{
    BIAS, BIGTENS, BLETCH, BNDRY_MASK, EXP_11, EXP_MASK, EXP_MSK1, EXP_SHIFT1, FRAC_MASK1, INT_MAX,
    LOG2P, N_BIGTENS, P, QUICK_MAX, SIGN_BIT, TEN_PMAX, TENS, w0_and, w0_or, w0_sub, word0, word1,
};
use crate::ctx::{BigRef, DtoaCtx};
use crate::{Digits, DtoaError, Kind, Rounding};

/// `dtoa`: the donor's double digit generator.
#[allow(unused_assignments)] // the donor's "pacify gcc" initialisations are kept verbatim
pub(crate) fn dtoa(
    ctx: &mut DtoaCtx,
    d0: f64,
    mode: i32,
    ndigits: i32,
    rounding: Rounding,
    out: &mut [u8],
) -> Result<Digits, DtoaError> {
    let mut d = d0;
    let negative;
    if word0(d) & SIGN_BIT != 0 {
        negative = true;
        w0_and(&mut d, !SIGN_BIT);
    } else {
        negative = false;
    }

    if word0(d) & EXP_MASK == EXP_MASK {
        if word1(d) == 0 && word0(d) & 0x000f_ffff == 0 {
            return Ok(Digits {
                len: 0,
                decpt: 0,
                negative,
                kind: Kind::Inf,
            });
        }
        return Ok(Digits {
            len: 0,
            decpt: 0,
            negative,
            kind: Kind::Nan,
        });
    }
    if d == 0.0 {
        if out.is_empty() {
            return Err(DtoaError::BufferTooSmall { needed: 1 });
        }
        out[0] = b'0';
        return Ok(Digits {
            len: 1,
            decpt: 1,
            negative,
            kind: Kind::Zero,
        });
    }

    let mut rounding = rounding;
    if matches!(rounding, Rounding::Upward | Rounding::Downward) {
        if negative {
            rounding = if rounding == Rounding::Upward {
                Rounding::TowardZero
            } else {
                Rounding::Upward
            };
        } else if rounding != Rounding::Upward {
            rounding = Rounding::TowardZero;
        }
    }

    let mut be = 0i32;
    let mut bbits = 0i32;
    let mut b = crate::bigint::d2b(ctx, d, &mut be, &mut bbits).ok_or(DtoaError::NoMemory)?;
    let mut s_ref = BigRef::NIL;
    let mut mhi = BigRef::NIL;
    let mut mlo = BigRef::NIL;

    let mut i = ((word0(d) >> EXP_SHIFT1) & (EXP_MASK >> EXP_SHIFT1)) as i32;
    let mut d2: f64;
    let denorm: bool;
    if i != 0 {
        d2 = d;
        w0_and(&mut d2, FRAC_MASK1);
        w0_or(&mut d2, EXP_11);
        i = i.wrapping_sub(BIAS);
        denorm = false;
    } else {
        i = bbits
            .wrapping_add(be)
            .wrapping_add(BIAS.wrapping_add(P.wrapping_sub(1)).wrapping_sub(1));
        let x: u32 = if i > 32 {
            (word0(d) << (64i32.wrapping_sub(i) as u32)) | (word1(d) >> (i.wrapping_sub(32) as u32))
        } else {
            word1(d) << (32i32.wrapping_sub(i) as u32)
        };
        d2 = f64::from(x);
        w0_sub(&mut d2, 31u32.wrapping_mul(EXP_MSK1));
        i = i.wrapping_sub(
            BIAS.wrapping_add(P.wrapping_sub(1))
                .wrapping_sub(1)
                .wrapping_add(1),
        );
        denorm = true;
    }
    let mut ds = (d2 - 1.5) * 0.289529654602168
        + 0.1760912590558
        + (i as f64) * f64::from_bits(0x3fd3_4413_509f_79fb);
    let mut k = ds as i32;
    if ds < 0.0 && ds != f64::from(k) {
        k = k.wrapping_sub(1);
    }
    let mut k_check = true;
    if (0..=TEN_PMAX).contains(&k) {
        if d < TENS[k as usize] {
            k = k.wrapping_sub(1);
        }
        k_check = false;
    }
    let j = bbits.wrapping_sub(i).wrapping_sub(1);
    let mut b2;
    let mut s2;
    if j >= 0 {
        b2 = 0;
        s2 = j;
    } else {
        b2 = j.wrapping_neg();
        s2 = 0;
    }
    let b5;
    let s5;
    if k >= 0 {
        b5 = 0;
        s5 = k;
        s2 = s2.wrapping_add(k);
    } else {
        b2 = b2.wrapping_sub(k);
        b5 = k.wrapping_neg();
        s5 = 0;
    }
    let mut mode = mode;
    if !(0..=9).contains(&mode) {
        mode = 0;
    }
    let mut try_quick = rounding == Rounding::Nearest;
    if mode > 5 {
        mode = mode.wrapping_sub(4);
        try_quick = false;
    }
    let mut leftright = true;
    let mut ilim = -1i32;
    let mut ilim1 = -1i32;
    let mut ndigits = ndigits;
    match mode {
        0 | 1 => {
            i = 18;
            ndigits = 0;
        }
        2 => {
            leftright = false;
            if ndigits <= 0 {
                ndigits = 1;
            }
            ilim = ndigits;
            ilim1 = ndigits;
            i = ndigits;
        }
        4 => {
            if ndigits <= 0 {
                ndigits = 1;
            }
            ilim = ndigits;
            ilim1 = ndigits;
            i = ndigits;
        }
        3 => {
            leftright = false;
            i = ndigits.wrapping_add(k).wrapping_add(1);
            ilim = i;
            ilim1 = i.wrapping_sub(1);
            if i <= 0 {
                i = 1;
            }
        }
        5 => {
            i = ndigits.wrapping_add(k).wrapping_add(1);
            ilim = i;
            ilim1 = i.wrapping_sub(1);
            if i <= 0 {
                i = 1;
            }
        }
        _ => {}
    }
    let cap = usize::try_from(i).unwrap_or(0);
    if cap > out.len() {
        return Err(DtoaError::BufferTooSmall { needed: cap });
    }
    let mut pos = 0usize;

    // The donor's mode-5 left/right setup passes a negative exponent to pow5mult when
    // `ilim < 0` and loops forever; the defined fcvt-style result is no digits. Return it early
    // (the donor's own `no_digits` result, with dtoa's empty-string "0" rule).
    if ilim < 0 && (mode == 3 || mode == 5) {
        ctx.bfree(b);
        if out.is_empty() {
            return Err(DtoaError::BufferTooSmall { needed: 1 });
        }
        out[0] = b'0';
        return Ok(Digits {
            len: 1,
            decpt: 1,
            negative,
            kind: Kind::Finite,
        });
    }

    if mode > 1 && rounding != Rounding::Nearest {
        leftright = false;
    }

    if (0..=QUICK_MAX).contains(&ilim) && try_quick {
        let d_saved = d;
        let k0 = k;
        let ilim0 = ilim;
        'quick: {
            i = 0;
            d2 = d;
            let mut ieps = 2i32;
            if k > 0 {
                ds = TENS[(k & 0xf) as usize];
                let mut j = (k as u32) >> 4;
                if j & (BLETCH as u32) != 0 {
                    j &= (BLETCH as u32).wrapping_sub(1);
                    d /= BIGTENS[N_BIGTENS - 1];
                    ieps += 1;
                }
                while j != 0 {
                    if j & 1 != 0 {
                        ieps += 1;
                        ds *= BIGTENS[i as usize];
                    }
                    j >>= 1;
                    i += 1;
                }
                d /= ds;
            } else {
                let jj1 = k.wrapping_neg();
                if jj1 != 0 {
                    d *= TENS[(jj1 & 0xf) as usize];
                    let mut j = (jj1 as u32) >> 4;
                    while j != 0 {
                        if j & 1 != 0 {
                            ieps += 1;
                            d *= BIGTENS[i as usize];
                        }
                        j >>= 1;
                        i += 1;
                    }
                }
            }
            if k_check && d < 1.0 && ilim > 0 {
                if ilim1 <= 0 {
                    break 'quick;
                }
                ilim = ilim1;
                k = k.wrapping_sub(1);
                d *= 10.0;
                ieps += 1;
            }
            let mut eps = f64::from(ieps) * d + 7.0;
            w0_sub(&mut eps, (P as u32 - 1).wrapping_mul(EXP_MSK1));
            if ilim == 0 {
                s_ref = BigRef::NIL;
                mhi = BigRef::NIL;
                d -= 5.0;
                if d > eps {
                    out[pos] = b'1';
                    pos += 1;
                    k = k.wrapping_add(1);
                    return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
                }
                if d < -eps {
                    k = ndigits.wrapping_neg().wrapping_sub(1);
                    return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
                }
                break 'quick;
            }
            if leftright {
                eps = 0.5 / TENS[(ilim - 1) as usize] - eps;
                i = 0;
                loop {
                    let l = d as i32;
                    d -= f64::from(l);
                    out[pos] = b'0' + (l as u8);
                    pos += 1;
                    if d < eps {
                        return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
                    }
                    if 1.0 - d < eps {
                        round_up(out, &mut pos, &mut k);
                        return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
                    }
                    i += 1;
                    if i >= ilim {
                        break;
                    }
                    eps *= 10.0;
                    d *= 10.0;
                }
            } else {
                eps *= TENS[(ilim - 1) as usize];
                i = 1;
                loop {
                    let l = d as i32;
                    d -= f64::from(l);
                    if d == 0.0 {
                        ilim = i;
                    }
                    out[pos] = b'0' + (l as u8);
                    pos += 1;
                    if i == ilim {
                        if d > 0.5 + eps {
                            round_up(out, &mut pos, &mut k);
                            return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
                        } else if d < 0.5 - eps {
                            trim_zeros(out, &mut pos);
                            return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
                        }
                        break 'quick;
                    }
                    d *= 10.0;
                    i += 1;
                }
            }
        }
        pos = 0;
        d = d_saved;
        k = k0;
        ilim = ilim0;
    }

    if be >= 0 && k <= INT_MAX {
        ds = TENS[k as usize];
        if ndigits < 0 && ilim <= 0 {
            s_ref = BigRef::NIL;
            mhi = BigRef::NIL;
            if ilim < 0 || d <= 5.0 * ds {
                k = ndigits.wrapping_neg().wrapping_sub(1);
                return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
            }
            out[pos] = b'1';
            pos += 1;
            k = k.wrapping_add(1);
            return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
        }
        let mut i = 1i32;
        loop {
            let mut l = (d / ds) as i32;
            d -= f64::from(l) * ds;
            if d < 0.0 {
                l = l.wrapping_sub(1);
                d += ds;
            }
            out[pos] = b'0' + (l as u8);
            pos += 1;
            if d == 0.0 {
                break;
            }
            if i == ilim {
                if mode > 1 {
                    match rounding {
                        Rounding::TowardZero => {
                            return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
                        }
                        Rounding::Upward => {
                            round_up(out, &mut pos, &mut k);
                            return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
                        }
                        _ => {}
                    }
                }
                d += d;
                if d > ds || (d == ds && (l & 1) != 0) {
                    round_up(out, &mut pos, &mut k);
                }
                break;
            }
            d *= 10.0;
            i += 1;
        }
        return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
    }

    let mut m2 = b2;
    let m5 = b5;
    let mut spec_case = false;
    if leftright {
        i = if denorm {
            be.wrapping_add(BIAS.wrapping_add(P.wrapping_sub(1)).wrapping_sub(1))
                .wrapping_add(1)
        } else {
            1i32.wrapping_add(P).wrapping_sub(bbits)
        };
        b2 = b2.wrapping_add(i);
        s2 = s2.wrapping_add(i);
        mhi = crate::bigint::i2b(ctx, 1).ok_or(DtoaError::NoMemory)?;
    }
    if m2 > 0 && s2 > 0 {
        i = if m2 < s2 { m2 } else { s2 };
        b2 = b2.wrapping_sub(i);
        m2 = m2.wrapping_sub(i);
        s2 = s2.wrapping_sub(i);
    }
    if b5 > 0 {
        if leftright {
            if m5 > 0 {
                mhi = crate::bigint::pow5mult(ctx, mhi, m5).ok_or(DtoaError::NoMemory)?;
                let b1 = crate::bigint::mult(ctx, mhi, b).ok_or(DtoaError::NoMemory)?;
                ctx.bfree(b);
                b = b1;
            }
            let j = b5.wrapping_sub(m5);
            if j != 0 {
                b = crate::bigint::pow5mult(ctx, b, j).ok_or(DtoaError::NoMemory)?;
            }
        } else {
            b = crate::bigint::pow5mult(ctx, b, b5).ok_or(DtoaError::NoMemory)?;
        }
    }
    let mut s_alloc = crate::bigint::i2b(ctx, 1).ok_or(DtoaError::NoMemory)?;
    s_ref = s_alloc;
    if s5 > 0 {
        s_alloc = crate::bigint::pow5mult(ctx, s_alloc, s5).ok_or(DtoaError::NoMemory)?;
        s_ref = s_alloc;
    }

    if (mode < 2 || leftright)
        && rounding == Rounding::Nearest
        && word1(d) == 0
        && word0(d) & BNDRY_MASK == 0
        && word0(d) & (EXP_MASK & !EXP_MSK1) != 0
    {
        b2 = b2.wrapping_add(LOG2P);
        s2 = s2.wrapping_add(LOG2P);
        spec_case = true;
    }

    i = ((if s5 != 0 {
        32i32.wrapping_sub(crate::bigint::hi0bits(
            ctx.x(s_alloc, (ctx.wds(s_alloc) - 1) as u32),
        ))
    } else {
        1
    })
    .wrapping_add(s2))
        & 0x1f;
    if i != 0 {
        i = 32i32.wrapping_sub(i);
    }
    if i > 4 {
        i = i.wrapping_sub(4);
        b2 = b2.wrapping_add(i);
        m2 = m2.wrapping_add(i);
        s2 = s2.wrapping_add(i);
    } else if i < 4 {
        i = i.wrapping_add(28);
        b2 = b2.wrapping_add(i);
        m2 = m2.wrapping_add(i);
        s2 = s2.wrapping_add(i);
    }
    if b2 > 0 {
        b = crate::bigint::lshift(ctx, b, b2).ok_or(DtoaError::NoMemory)?;
    }
    if s2 > 0 {
        s_alloc = crate::bigint::lshift(ctx, s_alloc, s2).ok_or(DtoaError::NoMemory)?;
        s_ref = s_alloc;
    }
    if k_check && crate::bigint::cmp(ctx, b, s_alloc) < 0 {
        k = k.wrapping_sub(1);
        b = crate::bigint::multadd(ctx, b, 10, 0).ok_or(DtoaError::NoMemory)?;
        if leftright {
            mhi = crate::bigint::multadd(ctx, mhi, 10, 0).ok_or(DtoaError::NoMemory)?;
        }
        ilim = ilim1;
    }
    if ilim <= 0 && (mode == 3 || mode == 5) {
        if ilim < 0 || {
            let s5v = crate::bigint::multadd(ctx, s_alloc, 5, 0).ok_or(DtoaError::NoMemory)?;
            s_alloc = s5v;
            s_ref = s_alloc;
            crate::bigint::cmp(ctx, b, s_alloc) <= 0
        } {
            k = ndigits.wrapping_neg().wrapping_sub(1);
            return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
        }
        out[pos] = b'1';
        pos += 1;
        k = k.wrapping_add(1);
        return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
    }
    let mut dig = b'0';
    if leftright {
        if m2 > 0 {
            mhi = crate::bigint::lshift(ctx, mhi, m2).ok_or(DtoaError::NoMemory)?;
        }
        mlo = mhi;
        if spec_case {
            let new_mhi = ctx.balloc(ctx.k(mhi)).ok_or(DtoaError::NoMemory)?;
            mhi = new_mhi;
            crate::bigint::bcopy(ctx, mhi, mlo);
            mhi = crate::bigint::lshift(ctx, mhi, LOG2P).ok_or(DtoaError::NoMemory)?;
        }
        let mut i = 1i32;
        loop {
            dig = (crate::bigint::quorem(ctx, b, s_alloc) as u8).wrapping_add(b'0');
            let j = crate::bigint::cmp(ctx, b, mlo);
            let delta = crate::bigint::diff(ctx, s_alloc, mhi).ok_or(DtoaError::NoMemory)?;
            let jj1 = if ctx.sign(delta) != 0 {
                1
            } else {
                crate::bigint::cmp(ctx, b, delta)
            };
            ctx.bfree(delta);
            if jj1 == 0 && mode != 1 && (word1(d) & 1) == 0 && rounding != Rounding::TowardZero {
                if dig == b'9' {
                    out[pos] = b'9';
                    pos += 1;
                    return roundoff(ctx, b, s_alloc, mhi, mlo, pos, k, negative, out);
                }
                if j > 0 {
                    dig = dig.wrapping_add(1);
                }
                out[pos] = dig;
                pos += 1;
                return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
            }
            if j < 0 || (j == 0 && mode != 1 && (word1(d) & 1) == 0) {
                if ctx.wds(b) <= 1 && ctx.x(b, 0) == 0 {
                    out[pos] = dig;
                    pos += 1;
                    return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
                }
                if mode > 1 {
                    match rounding {
                        Rounding::TowardZero => {
                            out[pos] = dig;
                            pos += 1;
                            return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
                        }
                        Rounding::Upward => {
                            out[pos] = dig;
                            pos += 1;
                            if i == ilim {
                                break;
                            }
                            b = crate::bigint::multadd(ctx, b, 10, 0).ok_or(DtoaError::NoMemory)?;
                            if mlo == mhi {
                                let new_m = crate::bigint::multadd(ctx, mhi, 10, 0)
                                    .ok_or(DtoaError::NoMemory)?;
                                mlo = new_m;
                                mhi = new_m;
                            } else {
                                mlo = crate::bigint::multadd(ctx, mlo, 10, 0)
                                    .ok_or(DtoaError::NoMemory)?;
                                mhi = crate::bigint::multadd(ctx, mhi, 10, 0)
                                    .ok_or(DtoaError::NoMemory)?;
                            }
                            i += 1;
                            continue;
                        }
                        _ => {}
                    }
                }
                if jj1 > 0 {
                    b = crate::bigint::lshift(ctx, b, 1).ok_or(DtoaError::NoMemory)?;
                    let jj1b = crate::bigint::cmp(ctx, b, s_alloc);
                    if jj1b > 0 || (jj1b == 0 && (dig & 1) != 0) {
                        let was_nine = dig == b'9';
                        dig = dig.wrapping_add(1);
                        if was_nine {
                            out[pos] = b'9';
                            pos += 1;
                            return roundoff(ctx, b, s_alloc, mhi, mlo, pos, k, negative, out);
                        }
                    }
                }
                out[pos] = dig;
                pos += 1;
                return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
            }
            if jj1 > 0 {
                if rounding == Rounding::TowardZero {
                    out[pos] = dig;
                    pos += 1;
                    return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
                }
                if dig == b'9' {
                    out[pos] = b'9';
                    pos += 1;
                    return roundoff(ctx, b, s_alloc, mhi, mlo, pos, k, negative, out);
                }
                out[pos] = dig.wrapping_add(1);
                pos += 1;
                return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
            }
            out[pos] = dig;
            pos += 1;
            if i == ilim {
                break;
            }
            b = crate::bigint::multadd(ctx, b, 10, 0).ok_or(DtoaError::NoMemory)?;
            if mlo == mhi {
                let new_m = crate::bigint::multadd(ctx, mhi, 10, 0).ok_or(DtoaError::NoMemory)?;
                mlo = new_m;
                mhi = new_m;
            } else {
                mlo = crate::bigint::multadd(ctx, mlo, 10, 0).ok_or(DtoaError::NoMemory)?;
                mhi = crate::bigint::multadd(ctx, mhi, 10, 0).ok_or(DtoaError::NoMemory)?;
            }
            i += 1;
        }
    } else {
        let mut i = 1i32;
        loop {
            dig = (crate::bigint::quorem(ctx, b, s_alloc) as u8).wrapping_add(b'0');
            out[pos] = dig;
            pos += 1;
            if ctx.wds(b) <= 1 && ctx.x(b, 0) == 0 {
                return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
            }
            if i >= ilim {
                break;
            }
            b = crate::bigint::multadd(ctx, b, 10, 0).ok_or(DtoaError::NoMemory)?;
            i += 1;
        }
    }

    match rounding {
        Rounding::TowardZero => {
            trim_zeros(out, &mut pos);
            return finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
        }
        Rounding::Upward => {
            return roundoff(ctx, b, s_ref, mhi, mlo, pos, k, negative, out);
        }
        _ => {}
    }
    b = crate::bigint::lshift(ctx, b, 1).ok_or(DtoaError::NoMemory)?;
    let j = crate::bigint::cmp(ctx, b, s_alloc);
    if j > 0 || (j == 0 && (dig & 1) != 0) {
        roundoff(ctx, b, s_ref, mhi, mlo, pos, k, negative, out)
    } else {
        trim_zeros(out, &mut pos);
        finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out)
    }
}

/// The donor's `bump_up` label: propagate a carry through the digit string.
fn round_up(out: &mut [u8], pos: &mut usize, k: &mut i32) {
    loop {
        *pos = pos.wrapping_sub(1);
        if out[*pos] == b'9' {
            if *pos == 0 {
                *k = k.wrapping_add(1);
                out[0] = b'0';
                break;
            }
        } else {
            break;
        }
    }
    out[*pos] = out[*pos].wrapping_add(1);
    *pos = pos.wrapping_add(1);
}

/// The donor's `trimzeros` label.
fn trim_zeros(out: &[u8], pos: &mut usize) {
    loop {
        if *pos == 0 {
            *pos = 1;
            break;
        }
        *pos = pos.wrapping_sub(1);
        if out[*pos] != b'0' {
            *pos = pos.wrapping_add(1);
            break;
        }
    }
}

/// The donor's `roundoff` label: carry the last digit up and finish.
#[allow(clippy::too_many_arguments)]
fn roundoff(
    ctx: &mut DtoaCtx,
    b: BigRef,
    s_ref: BigRef,
    mhi: BigRef,
    mlo: BigRef,
    mut pos: usize,
    k: i32,
    negative: bool,
    out: &mut [u8],
) -> Result<Digits, DtoaError> {
    let mut k = k;
    round_up(out, &mut pos, &mut k);
    finish(ctx, b, s_ref, mhi, mlo, pos, k, negative, out)
}

/// The shared `ret`/`ret1` epilogue: free every live block and build the result.
#[allow(clippy::too_many_arguments)]
fn finish(
    ctx: &mut DtoaCtx,
    b: BigRef,
    s_ref: BigRef,
    mhi: BigRef,
    mlo: BigRef,
    mut pos: usize,
    mut k: i32,
    negative: bool,
    out: &mut [u8],
) -> Result<Digits, DtoaError> {
    ctx.bfree(s_ref);
    if !mhi.is_nil() {
        if !mlo.is_nil() && mlo != mhi {
            ctx.bfree(mlo);
        }
        ctx.bfree(mhi);
    }
    ctx.bfree(b);
    if pos == 0 {
        if out.is_empty() {
            return Err(DtoaError::BufferTooSmall { needed: 1 });
        }
        out[0] = b'0';
        pos = 1;
        k = 0;
    }
    Ok(Digits {
        len: pos,
        decpt: k.wrapping_add(1),
        negative,
        kind: Kind::Finite,
    })
}

#[cfg(test)]
#[path = "dtoa_ut.rs"]
mod tests;
