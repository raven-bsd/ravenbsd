// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/internal/intscan.c and musl:src/internal/intscan.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// The donor files have no per-file header; they are covered by musl/COPYRIGHT (lines 1-24).
// MIT via musl/COPYRIGHT.
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//
//! The `scanf` integer lexer (`__intscan`): bases 2-36, prefix handling, saturation.
//!
//! `isspace`/`isdigit`/`isalpha` become ASCII helpers (`raven-libc-ctype` has the locale
//! classification; the scanf grammar is ASCII). `errno = ERANGE` is not set here: the engine
//! returns the saturated value and the shell owns `errno` (RD-23 keeps this as a private
//! copy until C41 exports `scan_integer`).

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The donor avoids overflow with explicit bounds before each multiply/add; the same
// inequalities are kept.

use crate::scanf::source::{Limited, ScanSource};
use crate::traits::CharUnit;

/// The donor's digit-value table: `0..=35` for a digit, `0xFF` for anything else.
fn digit_val(c: u32) -> u32 {
    if (0x30..=0x39).contains(&c) {
        c.wrapping_sub(0x30)
    } else if (0x61..=0x7a).contains(&c) {
        c.wrapping_sub(0x61).wrapping_add(10)
    } else if (0x41..=0x5a).contains(&c) {
        c.wrapping_sub(0x41).wrapping_add(10)
    } else {
        0xff
    }
}

fn is_space(c: u32) -> bool {
    matches!(c, 0x20 | 0x09 | 0x0a | 0x0b | 0x0c | 0x0d)
}

/// The log2 of a power-of-two base, selected by the donor's bit trick.
fn log2_base(base: u32) -> u32 {
    const T: [u32; 8] = [0, 1, 2, 4, 7, 3, 6, 5];
    let index = ((0x17u32.wrapping_mul(base)) >> 5) & 7;
    T.get(index as usize).copied().unwrap_or(0)
}

/// The donor's `__intscan`.
///
/// `base == 0` autodetects decimal, octal and hexadecimal; `pok` is false for `scanf`;
/// `lim` is `u64::MAX` for `scanf` (saturating).
pub(crate) fn intscan<C: CharUnit, S: ScanSource<C>>(
    f: &mut Limited<'_, S, C>,
    base: u32,
    pok: bool,
    lim: u64,
) -> u64 {
    if base > 36 || base == 1 {
        return 0;
    }
    let mut c = loop {
        let Some(c) = f.getc() else {
            f.shlim(0);
            return 0;
        };
        let v = c.to_u32();
        if !is_space(v) {
            break v;
        }
    };
    let mut neg = false;
    if c == 0x2b || c == 0x2d {
        neg = c == 0x2d;
        let Some(n) = f.getc() else {
            f.shlim(0);
            return 0;
        };
        c = n.to_u32();
    }
    let mut base = base;
    if (base == 0 || base == 16) && c == 0x30 {
        let Some(n) = f.getc() else {
            f.shlim(0);
            return 0;
        };
        c = n.to_u32();
        if (c | 0x20) == 0x78 {
            let Some(n) = f.getc() else {
                f.shlim(0);
                return 0;
            };
            c = n.to_u32();
            if digit_val(c) >= 16 {
                f.ungetc(C::from_u32(c));
                if pok {
                    f.ungetc(C::from_ascii(b'0'));
                } else {
                    f.shlim(0);
                }
                return 0;
            }
            base = 16;
        } else if base == 0 {
            base = 8;
        }
    } else {
        if base == 0 {
            base = 10;
        }
        if digit_val(c) >= base {
            f.ungetc(C::from_u32(c));
            f.shlim(0);
            return 0;
        }
    }

    let mut y: u64;
    if base == 10 {
        let mut x: u32 = 0;
        while c.wrapping_sub(0x30) < 10 && x < u32::MAX / 10 {
            x = x.saturating_mul(10).saturating_add(c.wrapping_sub(0x30));
            let Some(n) = f.getc() else {
                c = 0;
                break;
            };
            c = n.to_u32();
        }
        y = u64::from(x);
        while c.wrapping_sub(0x30) < 10
            && y <= u64::MAX / 10
            && 10u64.saturating_mul(y) <= u64::MAX.saturating_sub(c.wrapping_sub(0x30) as u64)
        {
            y = y
                .saturating_mul(10)
                .saturating_add(c.wrapping_sub(0x30) as u64);
            let Some(n) = f.getc() else {
                c = 0;
                break;
            };
            c = n.to_u32();
        }
        if c.wrapping_sub(0x30) < 10 {
            // fall through to the overflow path below
        }
    } else if base.is_power_of_two() {
        let bs = log2_base(base);
        let mut x: u32 = 0;
        while digit_val(c) < base && x <= u32::MAX / 32 {
            x = x.wrapping_shl(bs) | digit_val(c);
            let Some(n) = f.getc() else {
                c = 0;
                break;
            };
            c = n.to_u32();
        }
        y = u64::from(x);
        while digit_val(c) < base && y <= u64::MAX >> bs {
            y = y.wrapping_shl(bs) | u64::from(digit_val(c));
            let Some(n) = f.getc() else {
                c = 0;
                break;
            };
            c = n.to_u32();
        }
    } else {
        let mut x: u32 = 0;
        while digit_val(c) < base && x < u32::MAX / 36 {
            x = x.saturating_mul(base).saturating_add(digit_val(c));
            let Some(n) = f.getc() else {
                c = 0;
                break;
            };
            c = n.to_u32();
        }
        y = u64::from(x);
        while digit_val(c) < base
            && y <= u64::MAX / u64::from(base)
            && u64::from(base).saturating_mul(y) <= u64::MAX.saturating_sub(u64::from(digit_val(c)))
        {
            y = y
                .saturating_mul(u64::from(base))
                .saturating_add(u64::from(digit_val(c)));
            let Some(n) = f.getc() else {
                c = 0;
                break;
            };
            c = n.to_u32();
        }
    }
    if digit_val(c) < base {
        // Consume the rest of the digit run without storing it.
        while digit_val(c) < base {
            let Some(n) = f.getc() else {
                break;
            };
            c = n.to_u32();
        }
        y = lim;
        if lim & 1 != 0 {
            neg = false;
        }
    } else {
        f.ungetc(C::from_u32(c));
    }
    if y >= lim {
        if lim & 1 == 0 && !neg {
            return lim.saturating_sub(1);
        } else if y > lim {
            return lim;
        }
    }
    if neg { y.wrapping_neg() } else { y }
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "int_ut.rs"]
mod tests;
