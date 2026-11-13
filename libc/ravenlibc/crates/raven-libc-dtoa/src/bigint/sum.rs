// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/sum.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: sum.c,v 1.2 2008/03/21 23:13:48 christos Exp $)
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
//! `sum.c` of NetBSD gdtoa: `sum(a, b)` adds two big integers.
//!
//! Transpiled from `netbsd:lib/libc/gdtoa/sum.c` (David M. Gay, Lucent Technologies); see
//! `PROVENANCE.toml`.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::ctx::{BigRef, DtoaCtx};

/// `sum(a, b)`.
pub(crate) fn sum(ctx: &mut DtoaCtx, a: BigRef, b: BigRef) -> Option<BigRef> {
    let (a, b) = if ctx.wds(a) < ctx.wds(b) {
        (b, a)
    } else {
        (a, b)
    };
    let mut c = ctx.balloc(ctx.k(a))?;
    let wds_a = ctx.wds(a);
    let wds_b = ctx.wds(b);
    ctx.set_wds(c, wds_a);
    let mut carry = 0u64;
    let mut i = 0i32;
    while i < wds_b {
        let y = (ctx.x(a, i as u32) as u64)
            .wrapping_add(ctx.x(b, i as u32) as u64)
            .wrapping_add(carry);
        carry = y >> 32;
        ctx.set_x(c, i as u32, y as u32);
        i = i.wrapping_add(1);
    }
    while i < wds_a {
        let y = (ctx.x(a, i as u32) as u64).wrapping_add(carry);
        carry = y >> 32;
        ctx.set_x(c, i as u32, y as u32);
        i = i.wrapping_add(1);
    }
    if carry != 0 {
        if ctx.wds(c) == ctx.maxwds(c) {
            let bigger = ctx.balloc(ctx.k(c).wrapping_add(1))?;
            crate::bigint::bcopy(ctx, bigger, c);
            ctx.bfree(c);
            c = bigger;
        }
        let wds = ctx.wds(c);
        ctx.set_x(c, wds as u32, 1);
        ctx.set_wds(c, wds.wrapping_add(1));
    }
    Some(c)
}

#[cfg(test)]
#[path = "sum_ut.rs"]
mod tests;
