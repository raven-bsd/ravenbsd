// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/gmisc.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: gmisc.c,v 1.4 2012/03/13 21:13:34 christos Exp $)
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
//! `gmisc.c` of NetBSD gdtoa: `rshift` and `trailz`.
//!
//! Transpiled from `netbsd:lib/libc/gdtoa/gmisc.c` (David M. Gay, Lucent Technologies); see
//! `PROVENANCE.toml`. The NetBSD `_DIAGASSERT` is dropped (the count is bounded by `wds`).

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::config::{ALL_ON, KMASK, KSHIFT, UL_BITS};
use crate::ctx::{BigRef, DtoaCtx};

/// `rshift(b, k)`: shift right in place.
pub(crate) fn rshift(ctx: &mut DtoaCtx, b: BigRef, k: i32) {
    let wds = ctx.wds(b);
    let mut x1 = 0i32;
    let n = (k as u32) >> KSHIFT;
    if (n as i32) < wds {
        let mut x = n as i32;
        let kk = k & KMASK as i32;
        if kk != 0 {
            let nbits = UL_BITS.wrapping_sub(kk as u32);
            let mut y = ctx.x(b, x as u32) >> (kk as u32);
            x = x.wrapping_add(1);
            while x < wds {
                let value = (y | (ctx.x(b, x as u32) << nbits)) & ALL_ON;
                ctx.set_x(b, x1 as u32, value);
                y = ctx.x(b, x as u32) >> (kk as u32);
                x = x.wrapping_add(1);
                x1 = x1.wrapping_add(1);
            }
            ctx.set_x(b, x1 as u32, y);
            if y != 0 {
                x1 = x1.wrapping_add(1);
            }
        } else {
            while x < wds {
                ctx.set_x(b, x1 as u32, ctx.x(b, x as u32));
                x = x.wrapping_add(1);
                x1 = x1.wrapping_add(1);
            }
        }
    }
    ctx.set_wds(b, x1);
    if x1 == 0 {
        ctx.set_x(b, 0, 0);
    }
}

/// `trailz(b)`: the number of trailing zero bits.
pub(crate) fn trailz(ctx: &DtoaCtx, b: BigRef) -> i32 {
    let wds = ctx.wds(b);
    let mut n = 0i32;
    let mut x = 0i32;
    while x < wds && ctx.x(b, x as u32) == 0 {
        n = n.wrapping_add(UL_BITS as i32);
        x = x.wrapping_add(1);
    }
    if x < wds {
        let mut l = ctx.x(b, x as u32);
        n = n.wrapping_add(crate::bigint::lo0bits(&mut l));
    }
    n
}

#[cfg(test)]
#[path = "gmisc_ut.rs"]
mod tests;
