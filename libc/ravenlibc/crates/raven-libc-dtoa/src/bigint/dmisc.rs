// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/dmisc.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: dmisc.c,v 1.4 2008/03/21 23:13:48 christos Exp $)
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
//! `dmisc.c` of NetBSD gdtoa: the long-division step `quorem` of the digit generator.
//!
//! Transpiled from `netbsd:lib/libc/gdtoa/dmisc.c` (David M. Gay, Lucent Technologies); see
//! `PROVENANCE.toml`. `rv_alloc`, `nrv_alloc`, `freedtoa`, `strcp_D2A` and `memcpy_D2A` are not
//! needed: the result goes into the caller's buffer and slices copy themselves.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::ctx::{BigRef, DtoaCtx};

/// `quorem(b, S)`: divide `b` by `S` in place and return the quotient digit.
pub(crate) fn quorem(ctx: &mut DtoaCtx, b: BigRef, s: BigRef) -> i32 {
    let mut n = ctx.wds(s);
    if ctx.wds(b) < n {
        return 0;
    }
    n = n.wrapping_sub(1);
    let mut q = ctx.x(b, n as u32) / (ctx.x(s, n as u32).wrapping_add(1));
    if q != 0 {
        let mut borrow = 0u64;
        let mut carry = 0u64;
        let mut i = 0i32;
        loop {
            let ys = (ctx.x(s, i as u32) as u64)
                .wrapping_mul(q as u64)
                .wrapping_add(carry);
            carry = ys >> 32;
            let y = (ctx.x(b, i as u32) as u64)
                .wrapping_sub(ys & 0xffff_ffff)
                .wrapping_sub(borrow);
            borrow = (y >> 32) & 1;
            ctx.set_x(b, i as u32, y as u32);
            i = i.wrapping_add(1);
            if i > n {
                break;
            }
        }
        if ctx.x(b, n as u32) == 0 {
            let mut nn = n;
            while nn > 0 && ctx.x(b, nn as u32) == 0 {
                nn = nn.wrapping_sub(1);
            }
            ctx.set_wds(b, nn.wrapping_add(1));
        }
    }
    if crate::bigint::cmp(ctx, b, s) >= 0 {
        q = q.wrapping_add(1);
        let mut borrow = 0u64;
        let mut carry = 0u64;
        let mut i = 0i32;
        loop {
            let ys = (ctx.x(s, i as u32) as u64).wrapping_add(carry);
            carry = ys >> 32;
            let y = (ctx.x(b, i as u32) as u64)
                .wrapping_sub(ys & 0xffff_ffff)
                .wrapping_sub(borrow);
            borrow = (y >> 32) & 1;
            ctx.set_x(b, i as u32, y as u32);
            i = i.wrapping_add(1);
            if i > n {
                break;
            }
        }
        if ctx.x(b, n as u32) == 0 {
            let mut nn = n;
            while nn > 0 && ctx.x(b, nn as u32) == 0 {
                nn = nn.wrapping_sub(1);
            }
            ctx.set_wds(b, nn.wrapping_add(1));
        }
    }
    q as i32
}

#[cfg(test)]
#[path = "dmisc_ut.rs"]
mod tests;
