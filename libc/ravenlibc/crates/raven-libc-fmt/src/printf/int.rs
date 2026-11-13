// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990, 1993
//	The Regents of the University of California.  All rights reserved.
//
// This code is derived from software contributed to Berkeley by
// Chris Torek.
//
// Copyright (c) 2011 The FreeBSD Foundation
// Portions of this software were developed by David Chisnall
// under sponsorship from the FreeBSD Foundation.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/stdio/printfcommon.h @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! The donor's `__ultoa`/`__ujtoa` digit loops and `exponent` helper.
//!
//! `__ultoa` and `__ujtoa` are one loop in Rust because `uintmax_t` is `u64` on every
//! supported target. The donor's signed/unsigned split exists only to avoid a 64-bit
//! unsigned divide on 1990s machines and does not change the digits.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// Digit extraction is exactly the donor's unsigned arithmetic; the buffer is 64 bytes and
// the loop bound is the base-2 width, so no index is reachable outside it.

/// The donor's `to_digit`.
#[cfg(test)]
#[must_use]
pub(crate) const fn to_digit(c: u8) -> u32 {
    (c as u32).wrapping_sub(b'0' as u32)
}

/// The donor's `is_digit`.
#[cfg(test)]
#[must_use]
pub(crate) const fn is_digit(c: u32) -> bool {
    to_digit(c as u8) <= 9
}

/// The donor's `to_char`.
#[must_use]
pub(crate) const fn to_char(n: u32) -> u8 {
    (n as u8).wrapping_add(b'0')
}

/// Convert `val` to digits in `out`, returning the index of the first digit.
///
/// The donor's `__ultoa`: bases 2, 8, 10 and 16; `octzero` forces a leading zero for `%#o`;
/// `upper` selects `ABCDEF` for `%X`. The caller passes a 64-byte buffer (`BUF`).
pub(crate) fn ultoa(val: u64, out: &mut [u8; 64], base: u32, octzero: bool, upper: bool) -> usize {
    let mut cp = out.len();
    match base {
        10 => {
            let mut v = val;
            loop {
                cp = cp.wrapping_sub(1);
                out[cp] = to_char((v % 10) as u32);
                v /= 10;
                if v == 0 {
                    break;
                }
            }
        }
        2 => {
            let mut v = val;
            loop {
                cp = cp.wrapping_sub(1);
                out[cp] = to_char((v & 1) as u32);
                v >>= 1;
                if v == 0 {
                    break;
                }
            }
        }
        8 => {
            let mut v = val;
            loop {
                cp = cp.wrapping_sub(1);
                out[cp] = to_char((v & 7) as u32);
                v >>= 3;
                if v == 0 {
                    break;
                }
            }
            if octzero && out.get(cp).copied().unwrap_or(b'0') != b'0' {
                cp = cp.wrapping_sub(1);
                out[cp] = b'0';
            }
        }
        16 => {
            let digs: &[u8; 16] = if upper {
                b"0123456789ABCDEF"
            } else {
                b"0123456789abcdef"
            };
            let mut v = val;
            loop {
                cp = cp.wrapping_sub(1);
                out[cp] = digs[(v & 15) as usize];
                v >>= 4;
                if v == 0 {
                    break;
                }
            }
        }
        _ => {
            // The donor aborts; the engine only calls the four bases above. An empty
            // result keeps the caller's `size` at zero instead of panicking.
        }
    }
    cp
}

/// The donor's `__ujtoa`: identical to [`ultoa`] for the `u64` `uintmax_t`.
pub(crate) fn ujtoa(val: u64, out: &mut [u8; 64], base: u32, octzero: bool, upper: bool) -> usize {
    ultoa(val, out, base, octzero, upper)
}

/// The donor's `exponent`: write `e+NN`/`p-N` into `out`, returning the length.
///
/// Decimal exponents are at least two digits; hexadecimal exponents may be one digit.
pub(crate) fn exponent(out: &mut [u8; 8], exp: i32, fmtch: u8) -> usize {
    let mut p = 0usize;
    out[p] = fmtch;
    p = p.wrapping_add(1);
    let mut e = exp;
    if e < 0 {
        e = e.wrapping_neg();
        out[p] = b'-';
        p = p.wrapping_add(1);
    } else {
        out[p] = b'+';
        p = p.wrapping_add(1);
    }
    let mut digits = [0u8; 8];
    let mut n = 0usize;
    let mut v = e as u32;
    loop {
        digits[n] = to_char(v % 10);
        n = n.wrapping_add(1);
        v /= 10;
        if v == 0 {
            break;
        }
    }
    if n == 1 && (fmtch == b'e' || fmtch == b'E') {
        out[p] = b'0';
        p = p.wrapping_add(1);
    }
    while n > 0 {
        n = n.wrapping_sub(1);
        out[p] = digits[n];
        p = p.wrapping_add(1);
    }
    p
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
