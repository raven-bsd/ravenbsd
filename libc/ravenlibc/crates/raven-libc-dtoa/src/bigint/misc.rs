// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998, 1999 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/misc.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: misc.c,v 1.11 2011/11/21 09:46:19 mlelstv Exp $)
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
//! `misc.c` of NetBSD gdtoa: allocation (in `ctx`), `multadd`, `mult`, `pow5mult`, shifts,
//! comparison, difference, `b2d`, `d2b`, `lo0bits` and `hi0bits`.
//!
//! Transpiled from `netbsd:lib/libc/gdtoa/misc.c` (David M. Gay, Lucent Technologies); see
//! `PROVENANCE.toml`. The global free list, private pool and `p5s` chain are the caller's
//! [`DtoaCtx`]; the result-string allocators (`rv_alloc`, `nrv_alloc`, `freedtoa`) are replaced by
//! the caller's output slice and are therefore absent.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::config::{BIAS, EXP_MSK1, EXP_SHIFT, P};
use crate::ctx::{BigRef, DtoaCtx};

/// Sentinel alias.
const NIL: u32 = u32::MAX;

/// The donor's `lo0bits`: count trailing zero bits and shift the word right; returns 32 for zero.
pub(crate) fn lo0bits(y: &mut u32) -> i32 {
    let mut x = *y;
    if x & 7 != 0 {
        if x & 1 != 0 {
            return 0;
        }
        if x & 2 != 0 {
            *y = x >> 1;
            return 1;
        }
        *y = x >> 2;
        return 2;
    }
    let mut k = 0;
    if x & 0xffff == 0 {
        k = 16;
        x >>= 16;
    }
    if x & 0xff == 0 {
        k += 8;
        x >>= 8;
    }
    if x & 0xf == 0 {
        k += 4;
        x >>= 4;
    }
    if x & 0x3 == 0 {
        k += 2;
        x >>= 2;
    }
    if x & 1 == 0 {
        k += 1;
        x >>= 1;
        if x == 0 {
            return 32;
        }
    }
    *y = x;
    k
}

/// The donor's `hi0bits_D2A`: count leading zero bits; returns 32 for zero.
pub(crate) fn hi0bits(x: u32) -> i32 {
    let mut x = x;
    let mut k = 0;
    if x & 0xffff_0000 == 0 {
        k = 16;
        x <<= 16;
    }
    if x & 0xff00_0000 == 0 {
        k += 8;
        x <<= 8;
    }
    if x & 0xf000_0000 == 0 {
        k += 4;
        x <<= 4;
    }
    if x & 0xc000_0000 == 0 {
        k += 2;
        x <<= 2;
    }
    if x & 0x8000_0000 == 0 {
        k += 1;
        if x & 0x4000_0000 == 0 {
            return 32;
        }
    }
    k
}

/// `multadd(b, m, a)`: multiply by `m` and add `a`.
pub(crate) fn multadd(ctx: &mut DtoaCtx, b: BigRef, m: i32, a: i32) -> Option<BigRef> {
    let mut b = b;
    let mut wds = ctx.wds(b);
    let mut i = 0i32;
    let mut carry = a as u32 as u64;
    let mm = (m as i64) as u64;
    loop {
        let y = (ctx.x(b, i as u32) as u64)
            .wrapping_mul(mm)
            .wrapping_add(carry);
        carry = y >> 32;
        ctx.set_x(b, i as u32, y as u32);
        i = i.wrapping_add(1);
        if i >= wds {
            break;
        }
    }
    if carry != 0 {
        if wds >= ctx.maxwds(b) {
            let b1 = ctx.balloc(ctx.k(b).wrapping_add(1))?;
            crate::bigint::bcopy(ctx, b1, b);
            ctx.bfree(b);
            b = b1;
        }
        ctx.set_x(b, wds as u32, carry as u32);
        wds = wds.wrapping_add(1);
        ctx.set_wds(b, wds);
    }
    Some(b)
}

/// `i2b(i)`: a one-word integer.
pub(crate) fn i2b(ctx: &mut DtoaCtx, i: i32) -> Option<BigRef> {
    let b = ctx.balloc(1)?;
    ctx.set_x(b, 0, i as u32);
    ctx.set_wds(b, 1);
    Some(b)
}

/// `mult(a, b)`: schoolbook product.
pub(crate) fn mult(ctx: &mut DtoaCtx, a: BigRef, b: BigRef) -> Option<BigRef> {
    let (mut a, mut b) = (a, b);
    if ctx.wds(a) < ctx.wds(b) {
        core::mem::swap(&mut a, &mut b);
    }
    let mut k = ctx.k(a);
    let wa = ctx.wds(a);
    let wb = ctx.wds(b);
    let mut wc = wa.wrapping_add(wb);
    if wc > ctx.maxwds(a) {
        k = k.wrapping_add(1);
    }
    let c = ctx.balloc(k)?;
    let mut i = 0i32;
    while i < wc {
        ctx.set_x(c, i as u32, 0);
        i = i.wrapping_add(1);
    }
    let mut xb = 0i32;
    while xb < wb {
        let y = ctx.x(b, xb as u32);
        if y != 0 {
            let mut carry = 0u64;
            let mut x = 0i32;
            while x < wa {
                let z = (ctx.x(a, x as u32) as u64)
                    .wrapping_mul(y as u64)
                    .wrapping_add(ctx.x(c, (xb.wrapping_add(x)) as u32) as u64)
                    .wrapping_add(carry);
                carry = z >> 32;
                ctx.set_x(c, (xb.wrapping_add(x)) as u32, z as u32);
                x = x.wrapping_add(1);
            }
            ctx.set_x(c, (xb.wrapping_add(wa)) as u32, carry as u32);
        }
        xb = xb.wrapping_add(1);
    }
    while wc > 0 && ctx.x(c, (wc - 1) as u32) == 0 {
        wc = wc.wrapping_sub(1);
    }
    ctx.set_wds(c, wc);
    Some(c)
}

/// `pow5mult(b, k)`: multiply by `5^k`, building the per-context `5^(4*2^i)` chain lazily.
pub(crate) fn pow5mult(ctx: &mut DtoaCtx, b: BigRef, k: i32) -> Option<BigRef> {
    const P05: [i32; 3] = [5, 25, 125];
    let mut b = b;
    // The donor's gdtoa `small_ilim` can leave m5 > b5 for ilim == 0 and then calls
    // pow5mult with a negative exponent (the C loops in the unsigned shift). That value is the
    // unused `mhi` bound of the no-digit path, so no multiplication is the intended effect.
    if k < 0 {
        return Some(b);
    }
    let shift = k & 3;
    if shift != 0 {
        b = multadd(ctx, b, P05[(shift - 1) as usize], 0)?;
    }
    let mut k = (k as u32) >> 2;
    if k == 0 {
        return Some(b);
    }
    let mut p5;
    let head = ctx.p5_head();
    if head == NIL {
        p5 = i2b(ctx, 625)?;
        ctx.set_p5_head(p5.0);
    } else {
        p5 = BigRef(head);
    }
    loop {
        if k & 1 != 0 {
            let b1 = mult(ctx, b, p5)?;
            ctx.bfree(b);
            b = b1;
        }
        k >>= 1;
        if k == 0 {
            break;
        }
        let next = ctx.link(p5);
        if next == NIL {
            let p51 = mult(ctx, p5, p5)?;
            ctx.set_link(p5, p51.0);
            p5 = p51;
        } else {
            p5 = BigRef(next);
        }
    }
    Some(b)
}

/// `lshift(b, k)`.
pub(crate) fn lshift(ctx: &mut DtoaCtx, b: BigRef, k: i32) -> Option<BigRef> {
    let n = (k as u32) >> crate::config::KSHIFT;
    let mut k1 = ctx.k(b);
    let mut n1 = (n as i32).wrapping_add(ctx.wds(b)).wrapping_add(1);
    let mut i = ctx.maxwds(b);
    while n1 > i {
        i <<= 1;
        k1 = k1.wrapping_add(1);
    }
    let b1 = ctx.balloc(k1)?;
    let mut x1 = 0i32;
    i = 0;
    while i < n as i32 {
        ctx.set_x(b1, x1 as u32, 0);
        x1 = x1.wrapping_add(1);
        i = i.wrapping_add(1);
    }
    let mut x = 0i32;
    let xe = ctx.wds(b);
    let kk = k & crate::config::KMASK as i32;
    if kk != 0 {
        let k1s = 32i32.wrapping_sub(kk);
        let mut z = 0u32;
        loop {
            let digit = (ctx.x(b, x as u32) << (kk as u32)) | z;
            ctx.set_x(b1, x1 as u32, digit);
            z = ctx.x(b, x as u32) >> (k1s as u32);
            x = x.wrapping_add(1);
            x1 = x1.wrapping_add(1);
            if x >= xe {
                break;
            }
        }
        ctx.set_x(b1, x1 as u32, z);
        if z != 0 {
            n1 = n1.wrapping_add(1);
        }
    } else {
        loop {
            ctx.set_x(b1, x1 as u32, ctx.x(b, x as u32));
            x = x.wrapping_add(1);
            x1 = x1.wrapping_add(1);
            if x >= xe {
                break;
            }
        }
    }
    ctx.set_wds(b1, n1.wrapping_sub(1));
    ctx.bfree(b);
    Some(b1)
}

/// `cmp(a, b)`.
pub(crate) fn cmp(ctx: &DtoaCtx, a: BigRef, b: BigRef) -> i32 {
    let i = ctx.wds(a);
    let j = ctx.wds(b);
    let d = i.wrapping_sub(j);
    if d != 0 {
        return d;
    }
    let mut wa = j;
    while wa > 0 {
        wa = wa.wrapping_sub(1);
        let xa = ctx.x(a, wa as u32);
        let xb = ctx.x(b, wa as u32);
        if xa != xb {
            return if xa < xb { -1 } else { 1 };
        }
    }
    0
}

/// `diff(a, b)`: `a - b` with the donor's sign convention (`c->sign = 1` when `a < b`).
pub(crate) fn diff(ctx: &mut DtoaCtx, a: BigRef, b: BigRef) -> Option<BigRef> {
    let mut a = a;
    let mut b = b;
    let i = cmp(ctx, a, b);
    if i == 0 {
        let c = ctx.balloc(0)?;
        ctx.set_wds(c, 1);
        ctx.set_x(c, 0, 0);
        return Some(c);
    }
    let sign = i32::from(i < 0);
    if i < 0 {
        core::mem::swap(&mut a, &mut b);
    }
    let c = ctx.balloc(ctx.k(a))?;
    ctx.set_sign(c, sign);
    let wa = ctx.wds(a);
    let wb = ctx.wds(b);
    let mut borrow = 0u64;
    let mut xa = 0i32;
    let mut xb = 0i32;
    let mut xc = 0i32;
    loop {
        let y = (ctx.x(a, xa as u32) as u64)
            .wrapping_sub(ctx.x(b, xb as u32) as u64)
            .wrapping_sub(borrow);
        borrow = (y >> 32) & 1;
        ctx.set_x(c, xc as u32, y as u32);
        xa = xa.wrapping_add(1);
        xb = xb.wrapping_add(1);
        xc = xc.wrapping_add(1);
        if xb >= wb {
            break;
        }
    }
    while xa < wa {
        let y = (ctx.x(a, xa as u32) as u64).wrapping_sub(borrow);
        borrow = (y >> 32) & 1;
        ctx.set_x(c, xc as u32, y as u32);
        xa = xa.wrapping_add(1);
        xc = xc.wrapping_add(1);
    }
    let mut wds = wa;
    let mut last = xc;
    while last > 0 {
        last = last.wrapping_sub(1);
        if ctx.x(c, last as u32) != 0 {
            break;
        }
        wds = wds.wrapping_sub(1);
    }
    ctx.set_wds(c, wds);
    Some(c)
}

/// `b2d(a, &e)`: the most significant 53 bits of `a` as a `double`.
pub(crate) fn b2d(ctx: &DtoaCtx, a: BigRef, e: &mut i32) -> f64 {
    let wds = ctx.wds(a);
    let mut xa = wds;
    xa = xa.wrapping_sub(1);
    let mut y = ctx.x(a, xa as u32);
    let k = hi0bits(y);
    *e = 32i32.wrapping_sub(k);
    let eb = 11i32;
    let d0;
    let d1;
    if k < eb {
        d0 = crate::config::EXP_1 | (y >> (eb.wrapping_sub(k) as u32));
        let w = if xa > 0 {
            xa = xa.wrapping_sub(1);
            ctx.x(a, xa as u32)
        } else {
            0
        };
        d1 = (y << ((32i32.wrapping_sub(eb).wrapping_add(k)) as u32))
            | (w >> (eb.wrapping_sub(k) as u32));
    } else {
        let z = if xa > 0 {
            xa = xa.wrapping_sub(1);
            ctx.x(a, xa as u32)
        } else {
            0
        };
        let k2 = k.wrapping_sub(eb);
        if k2 != 0 {
            d0 = crate::config::EXP_1 | (y << (k2 as u32)) | (z >> (32i32.wrapping_sub(k2) as u32));
            y = if xa > 0 {
                xa = xa.wrapping_sub(1);
                ctx.x(a, xa as u32)
            } else {
                0
            };
            d1 = (z << (k2 as u32)) | (y >> (32i32.wrapping_sub(k2) as u32));
        } else {
            d0 = crate::config::EXP_1 | y;
            d1 = z;
        }
    }
    crate::config::from_words(d0, d1)
}

/// `d2b(dd, &e, &bits)`: split a `double` into a big integer and a binary exponent.
pub(crate) fn d2b(ctx: &mut DtoaCtx, dd: f64, e: &mut i32, bits: &mut i32) -> Option<BigRef> {
    let mut d0 = crate::config::word0(dd);
    let d1 = crate::config::word1(dd);
    let b = ctx.balloc(1)?;
    let mut z = d0 & crate::config::FRAC_MASK;
    d0 &= 0x7fff_ffff;
    let de = (d0 >> EXP_SHIFT) as i32;
    if de != 0 {
        z |= EXP_MSK1;
    }
    let mut k;
    let wds;
    if d1 != 0 {
        let mut y = d1;
        k = lo0bits(&mut y);
        let x0;
        if k != 0 {
            x0 = y | (z << (32i32.wrapping_sub(k) as u32));
            z >>= k as u32;
        } else {
            x0 = y;
        }
        ctx.set_x(b, 0, x0);
        ctx.set_x(b, 1, z);
        wds = if z != 0 { 2 } else { 1 };
    } else {
        k = lo0bits(&mut z);
        ctx.set_x(b, 0, z);
        wds = 1;
        k = k.wrapping_add(32);
    }
    ctx.set_wds(b, wds);
    if de != 0 {
        *e = de
            .wrapping_sub(BIAS)
            .wrapping_sub(P.wrapping_sub(1))
            .wrapping_add(k);
        *bits = P.wrapping_sub(k);
    } else {
        *e = de
            .wrapping_sub(BIAS)
            .wrapping_sub(P.wrapping_sub(1))
            .wrapping_add(1)
            .wrapping_add(k);
        let i = wds;
        *bits = 32i32
            .wrapping_mul(i)
            .wrapping_sub(hi0bits(ctx.x(b, (i - 1) as u32)));
    }
    Some(b)
}

// `fivesbits[]` lives in `strtodg.rs` with its donor file.
