// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998, 1999 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/gdtoa.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: gdtoa.c,v 1.11 2024/01/20 14:52:47 christos Exp $)
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
//! `gdtoa.c` of NetBSD gdtoa: generic binary to decimal digits, driven by a [`Fpi`].
//!
//! The five donor shapes stay separate (A-num-1): `dtoa.rs` is the double fast file, `strtod.rs`
//! the double parser, `strtodg.rs` the generic parser, and this file the generic generator used by
//! `ftoa` (binary32), `ldtoa` (ld80/ld128) and `strtold`'s tests. Inf/NaN are [`crate::Kind`], not
//! the donor's `-32768`.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::config::{
    ALL_ON, BIGTENS, EMIN, EXP_11, EXP_MSK1, EXP_SHIFT, FRAC_MASK1, INT_MAX, KSHIFT, N_BIGTENS, P,
    QUICK_MAX, TEN_PMAX, TENS, w0_add, w0_and, w0_or, w0_sub,
};
use crate::ctx::{BigRef, DtoaCtx};
use crate::types::*;
use crate::{Digits, DtoaError, Kind, Rounding};

/// `bitstob`: the donor's conversion of the `bits` array into a `Bigint`.
fn bitstob(ctx: &mut DtoaCtx, bits: &[u32], nbits: i32, bbits: &mut i32) -> Option<BigRef> {
    let mut i = crate::config::UL_BITS;
    let mut k = 0i32;
    while i < nbits as u32 {
        i <<= 1;
        k = k.wrapping_add(1);
    }
    let b = ctx.balloc(k)?;
    let nwords = (((nbits as u32).wrapping_sub(1)) >> KSHIFT).wrapping_add(1);
    let mut x = 0u32;
    while x < nwords {
        let value = bits.get(x as usize).copied().unwrap_or(0) & ALL_ON;
        ctx.set_x(b, x, value);
        x = x.wrapping_add(1);
    }
    let mut i = x as i32;
    loop {
        i = i.wrapping_sub(1);
        if ctx.x(b, i as u32) != 0 {
            break;
        }
        if i == 0 {
            ctx.set_wds(b, 0);
            *bbits = 0;
            return Some(b);
        }
    }
    ctx.set_wds(b, i.wrapping_add(1));
    *bbits = i
        .wrapping_mul(32)
        .wrapping_add(32)
        .wrapping_sub(crate::bigint::hi0bits(ctx.x(b, i as u32)));
    Some(b)
}

/// The generic `gdtoa`: returns `(len, decpt)` and updates `kindp` with the inexact flags.
#[allow(unused_assignments, clippy::too_many_arguments)] // the donor's "pacify gcc" initialisations are kept verbatim
pub(crate) fn gdtoa(
    ctx: &mut DtoaCtx,
    fpi: &Fpi,
    be: i32,
    bits: &[u32],
    kindp: &mut i32,
    mode: i32,
    ndigits: i32,
    rounding: Rounding,
    out: &mut [u8],
) -> Result<(usize, i32), DtoaError> {
    let mut inex = 0i32;
    if *kindp & STRTOG_NOMEMORY != 0 {
        return Err(DtoaError::NoMemory);
    }
    *kindp &= !STRTOG_INEXACT;
    let kind = *kindp;
    match kind & STRTOG_RETMASK {
        STRTOG_ZERO => {
            if out.is_empty() {
                return Err(DtoaError::BufferTooSmall { needed: 1 });
            }
            out[0] = b'0';
            *kindp |= inex;
            return Ok((1, 1));
        }
        STRTOG_NORMAL | STRTOG_DENORMAL => {}
        STRTOG_INFINITE => return Ok((0, 0)),
        STRTOG_NAN => return Ok((0, 0)),
        _ => return Ok((0, 0)),
    }

    let nbits = fpi.nbits;
    let be0 = be;
    let mut be = be;
    let mut bbits = 0i32;
    let mut b = bitstob(ctx, bits, nbits, &mut bbits).ok_or(DtoaError::NoMemory)?;
    let mut s_ref = BigRef::NIL;
    let mut mhi = BigRef::NIL;
    let mut mlo = BigRef::NIL;
    let mut mhi1;

    let trail = crate::bigint::trailz(ctx, b);
    if trail != 0 {
        crate::bigint::rshift(ctx, b, trail);
        be = be.wrapping_add(trail);
        bbits = bbits.wrapping_sub(trail);
    }
    if ctx.wds(b) == 0 {
        ctx.bfree(b);
        if out.is_empty() {
            return Err(DtoaError::BufferTooSmall { needed: 1 });
        }
        out[0] = b'0';
        return Ok((1, 1));
    }

    let mut i = 0i32;
    let mut d = crate::bigint::b2d(ctx, b, &mut i);
    i = be.wrapping_add(bbits).wrapping_sub(1);
    w0_and(&mut d, FRAC_MASK1);
    w0_or(&mut d, EXP_11);

    let mut ds = (d - 1.5) * 0.289529654602168
        + 0.1760912590558
        + (i as f64) * f64::from_bits(0x3fd3_4413_509f_79fb);
    let mut j = i;
    if j < 0 {
        j = j.wrapping_neg();
    }
    j = j.wrapping_sub(1077);
    if j > 0 {
        ds += (j as f64) * 7e-17;
    }
    let mut k = ds as i32;
    if ds < 0.0 && ds != f64::from(k) {
        k = k.wrapping_sub(1);
    }
    let mut k_check = true;
    w0_add(
        &mut d,
        ((be.wrapping_add(bbits).wrapping_sub(1)) as u32) << EXP_SHIFT,
    );
    if (0..=TEN_PMAX).contains(&k) {
        if d < TENS[k as usize] {
            k = k.wrapping_sub(1);
        }
        k_check = false;
    }
    let mut b2;
    let mut s2;
    let j = bbits.wrapping_sub(i).wrapping_sub(1);
    if j >= 0 {
        b2 = 0;
        s2 = j;
    } else {
        b2 = j.wrapping_neg();
        s2 = 0;
    }
    let mut b5;
    let mut s5;
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
    let mut try_quick = true;
    if mode > 5 {
        mode = mode.wrapping_sub(4);
        try_quick = false;
    } else if i >= -4i32.wrapping_sub(EMIN) || i < EMIN {
        try_quick = false;
    }
    let mut leftright = true;
    let mut ilim = -1i32;
    let mut ilim1 = -1i32;
    let mut ndigits = ndigits;
    match mode {
        0 | 1 => {
            i = ((nbits as f64) * f64::from_bits(0x3fd3_4413_5547_5a32)) as i32 + 3;
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

    // See the note in `dtoa.rs`: mode 5 with `ilim < 0` hangs the donor in `pow5mult`; return the
    // donor's `no_digits` result early (gdtoa returns an empty digit string).
    if ilim < 0 && (mode == 3 || mode == 5) {
        ctx.bfree(b);
        *kindp |= STRTOG_INEXLO;
        return Ok((0, ndigits.wrapping_neg()));
    }

    let mut rdir = rounding.as_fpi().wrapping_sub(1);
    if rdir != 0 {
        if rdir < 0 {
            rdir = 2;
        }
        if kind & STRTOG_NEG != 0 {
            rdir = 3i32.wrapping_sub(rdir);
        }
    }

    if (0..=QUICK_MAX).contains(&ilim) && try_quick && rdir == 0 && k == 0 {
        let d_saved = d;
        let k0 = k;
        let ilim0 = ilim;
        'quick: {
            i = 0;
            let mut ieps = 2i32;
            if k > 0 {
                ds = TENS[(k & 0xf) as usize];
                let mut j = (k as u32) >> 4;
                if j & (crate::config::BLETCH as u32) != 0 {
                    j &= (crate::config::BLETCH as u32).wrapping_sub(1);
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
            } else {
                ds = 1.0;
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
                    inex = STRTOG_INEXHI;
                    return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
                }
                if d < -eps {
                    k = ndigits.wrapping_neg().wrapping_sub(1);
                    inex = STRTOG_INEXLO;
                    return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
                }
                break 'quick;
            }
            if leftright {
                eps = ds * 0.5 / TENS[(ilim - 1) as usize] - eps;
                i = 0;
                loop {
                    let l = (d / ds) as i32;
                    d -= f64::from(l) * ds;
                    out[pos] = b'0' + (l as u8);
                    pos += 1;
                    if d < eps {
                        if d != 0.0 {
                            inex = STRTOG_INEXLO;
                        }
                        return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
                    }
                    if ds - d < eps {
                        round_up(out, &mut pos, &mut k);
                        inex = STRTOG_INEXHI;
                        return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
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
                    let l = (d / ds) as i32;
                    if l != 0 {
                        d -= f64::from(l) * ds;
                    }
                    out[pos] = b'0' + (l as u8);
                    pos += 1;
                    if i == ilim {
                        ds *= 0.5;
                        if d > ds + eps {
                            round_up(out, &mut pos, &mut k);
                            inex = STRTOG_INEXHI;
                            return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
                        } else if d < ds - eps {
                            if d != 0.0 {
                                inex = STRTOG_INEXLO;
                            }
                            trim_zeros(out, &mut pos);
                            return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
                        }
                        break;
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
                inex = STRTOG_INEXLO;
                return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
            }
            inex = STRTOG_INEXHI;
            out[pos] = b'1';
            pos += 1;
            k = k.wrapping_add(1);
            return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
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
                if rdir != 0 {
                    if rdir == 1 {
                        round_up(out, &mut pos, &mut k);
                        inex = STRTOG_INEXHI;
                    } else {
                        inex = STRTOG_INEXLO;
                    }
                    return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
                }
                d += d;
                if d > ds || (d == ds && (l & 1) != 0) {
                    round_up(out, &mut pos, &mut k);
                    inex = STRTOG_INEXHI;
                } else {
                    inex = STRTOG_INEXLO;
                    trim_zeros(out, &mut pos);
                }
                break;
            }
            d *= 10.0;
            i += 1;
        }
        return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
    }

    let mut m2 = b2;
    let mut m5 = b5;
    let mut spec_case = false;
    if leftright {
        i = nbits.wrapping_sub(bbits);
        let denormal = be.wrapping_sub(i) < fpi.emin && mode != 3 && mode != 5;
        i = i.wrapping_add(1);
        if denormal {
            i = be.wrapping_sub(fpi.emin).wrapping_add(1);
            if mode >= 2 && ilim > 0 && ilim < i {
                i = small_ilim(&mut m5, &mut s5, &mut b5, &mut m2, ilim);
            }
        } else if mode >= 2 {
            i = small_ilim(&mut m5, &mut s5, &mut b5, &mut m2, ilim);
        }
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

    if mode < 2 && bbits == 1 && be0 > fpi.emin.wrapping_add(1) {
        b2 = b2.wrapping_add(1);
        s2 = s2.wrapping_add(1);
        spec_case = true;
    }

    i = ((if s5 != 0 {
        crate::bigint::hi0bits(ctx.x(s_alloc, (ctx.wds(s_alloc) - 1) as u32))
    } else {
        crate::config::UL_BITS.wrapping_sub(1) as i32
    })
    .wrapping_sub(s2)
    .wrapping_sub(4))
        & crate::config::KMASK as i32;
    m2 = m2.wrapping_add(i);
    b2 = b2.wrapping_add(i);
    if b2 > 0 {
        b = crate::bigint::lshift(ctx, b, b2).ok_or(DtoaError::NoMemory)?;
    }
    s2 = s2.wrapping_add(i);
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
    if ilim <= 0 && mode > 2 {
        if ilim < 0 || {
            let s5v = crate::bigint::multadd(ctx, s_alloc, 5, 0).ok_or(DtoaError::NoMemory)?;
            s_alloc = s5v;
            s_ref = s_alloc;
            crate::bigint::cmp(ctx, b, s_alloc) <= 0
        } {
            k = ndigits.wrapping_neg().wrapping_sub(1);
            inex = STRTOG_INEXLO;
            return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
        }
        inex = STRTOG_INEXHI;
        out[pos] = b'1';
        pos += 1;
        k = k.wrapping_add(1);
        return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
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
            mhi = crate::bigint::lshift(ctx, mhi, 1).ok_or(DtoaError::NoMemory)?;
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
            if jj1 == 0 && mode == 0 && (bits[0] & 1) == 0 && rdir == 0 {
                if dig == b'9' {
                    out[pos] = b'9';
                    pos += 1;
                    inex = STRTOG_INEXHI;
                    return roundoff(ctx, b, s_alloc, mhi, mlo, pos, k, inex, kindp, out);
                }
                if j <= 0 {
                    if ctx.wds(b) > 1 || ctx.x(b, 0) != 0 {
                        inex = STRTOG_INEXLO;
                    }
                } else {
                    dig = dig.wrapping_add(1);
                    inex = STRTOG_INEXHI;
                }
                out[pos] = dig;
                pos += 1;
                return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
            }
            if j < 0 || (j == 0 && mode == 0 && (bits[0] & 1) == 0) {
                if rdir != 0 && (ctx.wds(b) > 1 || ctx.x(b, 0) != 0) {
                    if rdir == 2 {
                        inex = STRTOG_INEXLO;
                        out[pos] = dig;
                        pos += 1;
                        return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
                    }
                    while crate::bigint::cmp(ctx, s_alloc, mhi) > 0 {
                        out[pos] = dig;
                        pos += 1;
                        mhi1 =
                            crate::bigint::multadd(ctx, mhi, 10, 0).ok_or(DtoaError::NoMemory)?;
                        if mlo == mhi {
                            mlo = mhi1;
                        }
                        mhi = mhi1;
                        b = crate::bigint::multadd(ctx, b, 10, 0).ok_or(DtoaError::NoMemory)?;
                        dig = (crate::bigint::quorem(ctx, b, s_alloc) as u8).wrapping_add(b'0');
                    }
                    let was_nine = dig == b'9';
                    dig = dig.wrapping_add(1);
                    if was_nine {
                        out[pos] = b'9';
                        pos += 1;
                        inex = STRTOG_INEXHI;
                        return roundoff(ctx, b, s_alloc, mhi, mlo, pos, k, inex, kindp, out);
                    }
                    inex = STRTOG_INEXHI;
                    out[pos] = dig;
                    pos += 1;
                    return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
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
                            inex = STRTOG_INEXHI;
                            return roundoff(ctx, b, s_alloc, mhi, mlo, pos, k, inex, kindp, out);
                        }
                        inex = STRTOG_INEXHI;
                    }
                }
                if ctx.wds(b) > 1 || ctx.x(b, 0) != 0 {
                    inex = STRTOG_INEXLO;
                }
                out[pos] = dig;
                pos += 1;
                return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
            }
            if jj1 > 0 && rdir != 2 {
                if dig == b'9' {
                    out[pos] = b'9';
                    pos += 1;
                    inex = STRTOG_INEXHI;
                    return roundoff(ctx, b, s_alloc, mhi, mlo, pos, k, inex, kindp, out);
                }
                inex = STRTOG_INEXHI;
                out[pos] = dig.wrapping_add(1);
                pos += 1;
                return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
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
            if i >= ilim {
                break;
            }
            b = crate::bigint::multadd(ctx, b, 10, 0).ok_or(DtoaError::NoMemory)?;
            i += 1;
        }
    }

    if rdir != 0 {
        if rdir == 2 || (ctx.wds(b) <= 1 && ctx.x(b, 0) == 0) {
            if ctx.wds(b) > 1 || ctx.x(b, 0) != 0 {
                inex = STRTOG_INEXLO;
            }
            trim_zeros(out, &mut pos);
            return finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
        }
        inex = STRTOG_INEXHI;
        return roundoff(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out);
    }
    b = crate::bigint::lshift(ctx, b, 1).ok_or(DtoaError::NoMemory)?;
    let j = crate::bigint::cmp(ctx, b, s_alloc);
    if j > 0 || (j == 0 && (dig & 1) != 0) {
        inex = STRTOG_INEXHI;
        roundoff(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out)
    } else {
        if ctx.wds(b) > 1 || ctx.x(b, 0) != 0 {
            inex = STRTOG_INEXLO;
        }
        trim_zeros(out, &mut pos);
        finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out)
    }
}

/// The donor's `small_ilim` label: adjust `m5/s5/b5`, and return the `i` used for `b2/s2`.
fn small_ilim(m5: &mut i32, s5: &mut i32, b5: &mut i32, m2: &mut i32, ilim: i32) -> i32 {
    let j = ilim.wrapping_sub(1);
    if *m5 >= j {
        *m5 = m5.wrapping_sub(j);
    } else {
        let rest = j.wrapping_sub(*m5);
        *s5 = s5.wrapping_add(rest);
        *b5 = b5.wrapping_add(rest);
        *m5 = 0;
    }
    let mut i = ilim;
    if i < 0 {
        *m2 = m2.wrapping_sub(i);
        i = 0;
    }
    i
}

/// The donor's `bump_up` label.
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

/// The donor's `clear_trailing0` label.
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

/// The donor's `roundoff` label.
#[allow(clippy::too_many_arguments)]
fn roundoff(
    ctx: &mut DtoaCtx,
    b: BigRef,
    s_ref: BigRef,
    mhi: BigRef,
    mlo: BigRef,
    mut pos: usize,
    k: i32,
    inex: i32,
    kindp: &mut i32,
    out: &mut [u8],
) -> Result<(usize, i32), DtoaError> {
    let mut k = k;
    round_up(out, &mut pos, &mut k);
    finish(ctx, b, s_ref, mhi, mlo, pos, k, inex, kindp, out)
}

/// Shared `ret`/`ret1` epilogue.
#[allow(clippy::too_many_arguments)]
fn finish(
    ctx: &mut DtoaCtx,
    b: BigRef,
    s_ref: BigRef,
    mhi: BigRef,
    mlo: BigRef,
    pos: usize,
    k: i32,
    inex: i32,
    kindp: &mut i32,
    _out: &[u8],
) -> Result<(usize, i32), DtoaError> {
    ctx.bfree(s_ref);
    if !mhi.is_nil() {
        if !mlo.is_nil() && mlo != mhi {
            ctx.bfree(mlo);
        }
        ctx.bfree(mhi);
    }
    ctx.bfree(b);
    *kindp |= inex;
    Ok((pos, k.wrapping_add(1)))
}

/// `ftoa`: the binary32 decomposition of `g_ffmt.c` (STUDY) through the generic `gdtoa` core.
pub(crate) fn ftoa(
    ctx: &mut DtoaCtx,
    v: f32,
    mode: i32,
    ndigits: i32,
    rounding: Rounding,
    out: &mut [u8],
) -> Result<Digits, DtoaError> {
    let raw = v.to_bits();
    let negative = raw & 0x8000_0000 != 0;
    let abs = raw & 0x7fff_ffff;
    if abs & 0x7f80_0000 == 0x7f80_0000 {
        let kind = if abs & 0x007f_ffff != 0 {
            Kind::Nan
        } else {
            Kind::Inf
        };
        return Ok(Digits {
            len: 0,
            decpt: 0,
            negative,
            kind,
        });
    }
    if abs == 0 {
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
    let mut bits = [0u32; 1];
    bits[0] = abs & 0x007f_ffff;
    let mut ex = ((abs >> 23) & 0xff) as i32;
    if ex != 0 {
        bits[0] |= 0x0080_0000;
    } else {
        ex = 1;
    }
    ex = ex.wrapping_sub(0x7f).wrapping_sub(23);
    let mut kind = STRTOG_NORMAL;
    if negative {
        kind |= STRTOG_NEG;
    }
    let fpi = Fpi::float().with_rounding(rounding.as_fpi());
    let (len, decpt) = gdtoa(
        ctx, &fpi, ex, &bits, &mut kind, mode, ndigits, rounding, out,
    )?;
    Ok(Digits {
        len,
        decpt,
        negative,
        kind: Kind::Finite,
    })
}

/// Kind conversion for the caller of [`gdtoa`].
#[must_use]
pub(crate) fn kind_of(flags: i32) -> Kind {
    match flags & STRTOG_RETMASK {
        STRTOG_ZERO => Kind::Zero,
        STRTOG_INFINITE => Kind::Inf,
        STRTOG_NAN | STRTOG_NANBITS => Kind::Nan,
        _ => Kind::Finite,
    }
}

#[cfg(test)]
#[path = "gdtoa_ut.rs"]
mod tests;
