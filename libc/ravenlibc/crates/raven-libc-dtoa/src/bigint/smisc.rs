// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998, 1999 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/smisc.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: smisc.c,v 1.5 2012/03/13 21:13:34 christos Exp $)
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
//! `smisc.c` of NetBSD gdtoa: `s2b` (decimal digits to a big integer), `ratio`, the
//! case-insensitive `match`, `copybits` and `any_on`.
//!
//! Transpiled from `netbsd:lib/libc/gdtoa/smisc.c` (David M. Gay, Lucent Technologies); see
//! `PROVENANCE.toml`.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::config::{EXP_MSK1, UL_BITS, at};
use crate::ctx::{BigRef, DtoaCtx};

/// `s2b(s, nd0, nd, y9, dplen)`: the integer made of the first `nd` digits.
pub(crate) fn s2b(
    ctx: &mut DtoaCtx,
    s: &[u8],
    nd0: i32,
    nd: i32,
    y9: u32,
    dplen: usize,
) -> Option<BigRef> {
    let x = (nd.wrapping_add(8)) / 9;
    let mut k = 0i32;
    let mut y = 1i32;
    while x > y {
        y <<= 1;
        k = k.wrapping_add(1);
    }
    let mut b = ctx.balloc(k)?;
    ctx.set_x(b, 0, y9);
    ctx.set_wds(b, 1);
    let mut i = 9i32;
    let mut si;
    if 9 < nd0 {
        si = 9usize;
        loop {
            b = crate::bigint::multadd(
                ctx,
                b,
                10,
                i32::from(at(s, si)).wrapping_sub(i32::from(b'0')),
            )?;
            si = si.wrapping_add(1);
            i = i.wrapping_add(1);
            if i >= nd0 {
                break;
            }
        }
        si = si.wrapping_add(dplen);
    } else {
        si = dplen.wrapping_add(9);
    }
    while i < nd {
        b = crate::bigint::multadd(
            ctx,
            b,
            10,
            i32::from(at(s, si)).wrapping_sub(i32::from(b'0')),
        )?;
        si = si.wrapping_add(1);
        i = i.wrapping_add(1);
    }
    Some(b)
}

/// `ratio(a, b)` as a `double`.
pub(crate) fn ratio(ctx: &DtoaCtx, a: BigRef, b: BigRef) -> f64 {
    let mut ka = 0i32;
    let mut kb = 0i32;
    let mut da = crate::bigint::b2d(ctx, a, &mut ka);
    let mut db = crate::bigint::b2d(ctx, b, &mut kb);
    let mut k = ka
        .wrapping_sub(kb)
        .wrapping_add((UL_BITS as i32).wrapping_mul(ctx.wds(a).wrapping_sub(ctx.wds(b))));
    if k > 0 {
        crate::config::w0_add(&mut da, (k as u32).wrapping_mul(EXP_MSK1));
    } else {
        k = k.wrapping_neg();
        crate::config::w0_add(&mut db, (k as u32).wrapping_mul(EXP_MSK1));
    }
    da / db
}

/// `match(sp, t)`: case-insensitive comparison of the bytes after `*pos` and commit `*pos`.
///
/// The donor's pointer starts at the letter before the first compared byte (`*++s`), so a match of
/// `b"nf"` against `"inf"` compares `n`,`f` and leaves `*pos` one past `f`. On failure `*pos` is
/// unchanged.
pub(crate) fn match_prefix(s: &[u8], pos: &mut usize, t: &[u8]) -> bool {
    let mut i = *pos;
    for &expected in t {
        i = i.wrapping_add(1);
        let mut c = i32::from(at(s, i));
        if (c as u8).is_ascii_uppercase() {
            c = c.wrapping_add(i32::from(b'a') - i32::from(b'A'));
        }
        if c != i32::from(expected) {
            return false;
        }
    }
    *pos = i.wrapping_add(1);
    true
}

/// `copybits(c, n, b)`: copy `b` into the first `n` bits of `c`, zero-filling.
pub(crate) fn copybits(c: &mut [u32], n: i32, b: BigRef, ctx: &DtoaCtx) {
    let ce = (((n.wrapping_sub(1)) as u32) >> crate::config::KSHIFT).wrapping_add(1) as usize;
    let wds = ctx.wds(b) as usize;
    let mut i = 0usize;
    while i < wds {
        if let Some(slot) = c.get_mut(i) {
            *slot = ctx.x(b, i as u32);
        }
        i = i.wrapping_add(1);
    }
    while i < ce {
        if let Some(slot) = c.get_mut(i) {
            *slot = 0;
        }
        i = i.wrapping_add(1);
    }
}

/// `any_on(b, k)`: whether any bit below `k` is set.
pub(crate) fn any_on(ctx: &DtoaCtx, b: BigRef, k: i32) -> u32 {
    let nwds = ctx.wds(b);
    let mut n = (k as u32 >> crate::config::KSHIFT) as i32;
    if n > nwds {
        n = nwds;
    } else if n < nwds && (k & crate::config::KMASK as i32) != 0 {
        let x2 = ctx.x(b, n as u32);
        let shift = (k & crate::config::KMASK as i32) as u32;
        let x1 = (x2 >> shift) << shift;
        if x1 != x2 {
            return 1;
        }
    }
    let mut i = n;
    while i > 0 {
        i = i.wrapping_sub(1);
        if ctx.x(b, i as u32) != 0 {
            return 1;
        }
    }
    0
}

#[cfg(test)]
#[path = "smisc_ut.rs"]
mod tests;
