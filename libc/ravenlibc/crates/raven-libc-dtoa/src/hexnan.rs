// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 2000 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/hexnan.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: hexnan.c,v 1.5 2011/03/21 04:52:09 christos Exp $)
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
//! `hexnan.c` of NetBSD gdtoa: the `nan(n-char-sequence)` payload.
//!
//! The donor's lazy `hexdig_init_D2A()` is the compile-time `HEXDIG` table.

use crate::config::{KMASK, KSHIFT, UL_BITS, at};
use crate::hexdig::HEXDIG;
use crate::types::*;

/// Read one word of the caller's `bits` array (out-of-range reads zero).
fn rd(x0: &[u32], index: usize) -> u32 {
    x0.get(index).copied().unwrap_or(0)
}

/// Write one word of the caller's `bits` array.
fn wr(x0: &mut [u32], index: usize, value: u32) {
    if let Some(slot) = x0.get_mut(index) {
        *slot = value;
    }
}

/// The donor's `L_shift`.
fn l_shift(x0: &mut [u32], from: usize, to: usize, i: i32) {
    let shift = 8i32.wrapping_sub(i).wrapping_mul(4);
    let j = UL_BITS.wrapping_sub(shift as u32);
    let mut index = from;
    while index < to {
        let value = rd(x0, index) | (rd(x0, index.wrapping_add(1)) << j);
        wr(x0, index, value);
        let next = rd(x0, index.wrapping_add(1)) >> shift as u32;
        wr(x0, index.wrapping_add(1), next);
        index = index.wrapping_add(1);
    }
}

/// `hexnan(sp, fpi, x0)`: parse a `nan(...)` payload and return `STRTOG_NaN` or `STRTOG_NaNbits`.
pub(crate) fn hexnan(input: &[u8], sp: &mut usize, fpi: &Fpi, x0: &mut [u32]) -> i32 {
    let nbits = fpi.nbits;
    let mut x = (nbits as u32 >> KSHIFT) as usize;
    if nbits & KMASK as i32 != 0 {
        x = x.wrapping_add(1);
    }
    x = x.wrapping_sub(1);
    wr(x0, x, 0);
    let mut x1 = x;
    let xe = x;
    let mut havedig = 0i32;
    let mut hd0 = 0i32;
    let mut i = 0i32;
    let mut s = *sp;
    while {
        let next = at(input, s.wrapping_add(1));
        next != 0 && next <= b' '
    } {
        s = s.wrapping_add(1);
    }
    if at(input, s.wrapping_add(1)) == b'0'
        && (at(input, s.wrapping_add(2)) == b'x' || at(input, s.wrapping_add(2)) == b'X')
        && at(input, s.wrapping_add(3)) > b' '
    {
        s = s.wrapping_add(2);
    }
    loop {
        s = s.wrapping_add(1);
        let c = at(input, s);
        if c == 0 {
            break;
        }
        let h = HEXDIG.get(usize::from(c)).copied().unwrap_or(0);
        if h == 0 {
            if c <= b' ' {
                if hd0 < havedig {
                    if x < x1 && i < 8 {
                        l_shift(x0, x, x1, i);
                    }
                    if x == 0 {
                        i = 8;
                        continue;
                    }
                    hd0 = havedig;
                    x = x.wrapping_sub(1);
                    wr(x0, x, 0);
                    x1 = x;
                    i = 0;
                }
                while {
                    let next = at(input, s.wrapping_add(1));
                    next != 0 && next <= b' '
                } {
                    s = s.wrapping_add(1);
                }
                if at(input, s.wrapping_add(1)) == b'0'
                    && (at(input, s.wrapping_add(2)) == b'x'
                        || at(input, s.wrapping_add(2)) == b'X')
                    && at(input, s.wrapping_add(3)) > b' '
                {
                    s = s.wrapping_add(2);
                }
                continue;
            }
            if c == b')' && havedig != 0 {
                *sp = s.wrapping_add(1);
                break;
            }
            loop {
                s = s.wrapping_add(1);
                let next = at(input, s);
                if next == 0 {
                    break;
                }
                if next == b')' {
                    *sp = s.wrapping_add(1);
                    break;
                }
            }
            return STRTOG_NAN;
        }
        havedig = havedig.wrapping_add(1);
        i = i.wrapping_add(1);
        if i > 8 {
            if x == 0 {
                continue;
            }
            i = 1;
            x = x.wrapping_sub(1);
            wr(x0, x, 0);
        }
        let value = (rd(x0, x) << 4) | u32::from(h & 0x0f);
        wr(x0, x, value);
    }
    if havedig == 0 {
        return STRTOG_NAN;
    }
    if x < x1 && i < 8 {
        l_shift(x0, x, x1, i);
    }
    if x > 0 {
        let mut src = x;
        let mut dst = 0usize;
        loop {
            wr(x0, dst, rd(x0, src));
            dst = dst.wrapping_add(1);
            src = src.wrapping_add(1);
            if src > xe {
                break;
            }
        }
        loop {
            wr(x0, dst, 0);
            dst = dst.wrapping_add(1);
            if dst > xe {
                break;
            }
        }
    } else {
        let j = nbits & KMASK as i32;
        if j != 0 {
            let value = rd(x0, xe) & (0xffff_ffffu32 >> (UL_BITS.wrapping_sub(j as u32)));
            wr(x0, xe, value);
        }
    }
    let mut index = xe;
    loop {
        if rd(x0, index) != 0 {
            break;
        }
        if index == 0 {
            wr(x0, 0, 1);
            break;
        }
        index = index.wrapping_sub(1);
    }
    STRTOG_NANBITS
}

#[cfg(test)]
#[path = "hexnan_ut.rs"]
mod tests;
