// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998, 1999 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/ulp.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: ulp.c,v 1.3 2011/03/20 23:15:35 christos Exp $)
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
//! `ulp.c` of NetBSD gdtoa: the size of one unit in the last place of a `double`.
//!
//! Transpiled from `netbsd:lib/libc/gdtoa/ulp.c` (David M. Gay, Lucent Technologies); see
//! `PROVENANCE.toml`. The `double` arithmetic is the donor's own (A-num-3): plain `f64` plus,
//! exactly IEEE on SSE2, AArch64, RISC-V and compiler-builtins soft float.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::config::{EXP_MASK, EXP_MSK1, EXP_SHIFT, P, from_words, word0};

/// `ulp(x)`.
pub(crate) fn ulp(x: f64) -> f64 {
    let l = (word0(x) & EXP_MASK).wrapping_sub((P as u32).wrapping_sub(1).wrapping_mul(EXP_MSK1))
        as i32;
    if l > 0 {
        return from_words(l as u32, 0);
    }
    let mut l = (l as u32).wrapping_neg() >> EXP_SHIFT;
    if l < EXP_SHIFT {
        return from_words(0x0008_0000 >> l, 0);
    }
    l = l.wrapping_sub(EXP_SHIFT);
    let w1 = if l >= 31 {
        1
    } else {
        1u32 << (31u32.wrapping_sub(l))
    };
    from_words(0, w1)
}

#[cfg(test)]
#[path = "ulp_ut.rs"]
mod tests;
