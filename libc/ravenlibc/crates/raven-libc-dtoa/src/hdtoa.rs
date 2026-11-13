// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Copyright (c) 2004, 2005 David Schultz <das@FreeBSD.ORG>
// All rights reserved.
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/hdtoa.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: hdtoa.c,v 1.14 2024/06/09 15:06:07 jakllsch Exp $)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//! `hdtoa.c` of NetBSD gdtoa: the `printf %a` hexadecimal digit generator.
//!
//! The donor's subnormal path multiplies by `0x1p514` and reads the exponent; this port normalises
//! with bit operations instead (no floating-point arithmetic at all, 5.3). `dorounding` honours the
//! [`Rounding`] parameter, not `FLT_ROUNDS` (A-num-8). `Inf`/`NaN` return `len == 0`; the donor's
//! `INT_MAX` decpt is hidden.
//!
//! The digits are the donor's: the point goes after the first digit and `exp2` is the donor's
//! `decpt - 1` (the exponent the FreeBSD `%a` code prints). For x87 the leading nibble carries the
//! explicit integer bit, so `0x8p-3` is the value 1.0 there.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// faithful numeric port; overflow and bounds excluded by the donor's argument; debug tests run with overflow-checks on

use crate::{DtoaError, HexDigits, Kind, LongDouble, Rounding};

/// The lower/upper hexadecimal digit alphabets of the donor callers.
const LOWER: [u8; 16] = *b"0123456789abcdef";
const UPPER: [u8; 16] = *b"0123456789ABCDEF";

/// `roundup`: carry through an `f...f` digit string; true when the exponent must be bumped.
fn roundup(digits: &mut [u8], ndigits: usize) -> bool {
    let mut s = ndigits.wrapping_sub(1);
    loop {
        if digits[s] == 0xf {
            if s == 0 {
                digits[0] = 1;
                return true;
            }
            digits[s] = 0;
        } else {
            digits[s] = digits[s].wrapping_add(1);
            return false;
        }
        s = s.wrapping_sub(1);
    }
}

/// `dorounding` merged into the digit generator.
fn dorounding(
    digits: &mut [u8],
    ndigits: usize,
    sigfigs: usize,
    negative: bool,
    rounding: Rounding,
    exp2: &mut i32,
) {
    if sigfigs <= ndigits || digits[ndigits] == 0 {
        return;
    }
    let should = match rounding {
        Rounding::TowardZero => false,
        Rounding::Nearest => {
            digits[ndigits] > 8 || (digits[ndigits] == 8 && digits[ndigits - 1] & 1 != 0)
        }
        Rounding::Upward => !negative,
        Rounding::Downward => negative,
    };
    if should && roundup(digits, ndigits) {
        *exp2 = exp2.wrapping_add(4);
    }
}

/// Generate the hexadecimal digits, pad/auto-size, round, and map to ASCII.
#[allow(clippy::too_many_arguments)]
fn hex_digits(
    sig: u128,
    sigfigs: usize,
    exp2: i32,
    negative: bool,
    ndigits: i32,
    rounding: Rounding,
    upper: bool,
    out: &mut [u8],
    kind: Kind,
) -> Result<HexDigits, DtoaError> {
    let requested = if ndigits == 0 { 1 } else { ndigits };
    let buffer = if requested < 0 {
        sigfigs
    } else {
        usize::try_from(requested)
            .unwrap_or(usize::MAX)
            .max(sigfigs)
    };
    if out.len() < buffer {
        return Err(DtoaError::BufferTooSmall { needed: buffer });
    }
    let mut i = 0usize;
    while i < sigfigs {
        let shift = (sigfigs.wrapping_sub(1).wrapping_sub(i)).wrapping_mul(4);
        let nibble = ((sig >> shift) & 0xf) as u8;
        out[i] = nibble;
        i = i.wrapping_add(1);
    }
    while i < buffer {
        out[i] = 0;
        i = i.wrapping_add(1);
    }
    let len = if requested < 0 {
        let mut n = sigfigs;
        while n > 1 && out[n - 1] == 0 {
            n = n.wrapping_sub(1);
        }
        n
    } else {
        requested as usize
    };
    let mut exp2 = exp2;
    dorounding(out, len, sigfigs, negative, rounding, &mut exp2);
    let alphabet = if upper { UPPER } else { LOWER };
    i = 0;
    while i < len {
        out[i] = alphabet[usize::from(out[i])];
        i = i.wrapping_add(1);
    }
    Ok(HexDigits {
        len,
        exp2,
        negative,
        kind,
    })
}

/// Zero result shared by both entries.
fn zero(negative: bool, out: &mut [u8]) -> Result<HexDigits, DtoaError> {
    if out.is_empty() {
        return Err(DtoaError::BufferTooSmall { needed: 1 });
    }
    out[0] = b'0';
    Ok(HexDigits {
        len: 1,
        exp2: 0,
        negative,
        kind: Kind::Zero,
    })
}

/// `hdtoa`: double to hexadecimal digits.
pub(crate) fn hdtoa(
    v: f64,
    upper: bool,
    ndigits: i32,
    rounding: Rounding,
    out: &mut [u8],
) -> Result<HexDigits, DtoaError> {
    let raw = v.to_bits();
    let negative = raw & 0x8000_0000_0000_0000 != 0;
    let exp = ((raw >> 52) & 0x7ff) as i32;
    let frac = raw & 0x000f_ffff_ffff_ffff;
    if exp == 0x7ff {
        let kind = if frac != 0 { Kind::Nan } else { Kind::Inf };
        return Ok(HexDigits {
            len: 0,
            exp2: 0,
            negative,
            kind,
        });
    }
    if exp == 0 && frac == 0 {
        return zero(negative, out);
    }
    let (sig, exp2) = if exp == 0 {
        let h = 63i32.wrapping_sub(frac.leading_zeros() as i32);
        (
            u128::from(frac) << (52i32.wrapping_sub(h) as u32),
            h.wrapping_sub(1074),
        )
    } else {
        ((1u128 << 52) | u128::from(frac), exp.wrapping_sub(1023))
    };
    hex_digits(
        sig,
        14,
        exp2,
        negative,
        ndigits,
        rounding,
        upper,
        out,
        Kind::Finite,
    )
}

/// `hldtoa`: C `long double` to hexadecimal digits.
pub(crate) fn hldtoa(
    v: LongDouble,
    upper: bool,
    ndigits: i32,
    rounding: Rounding,
    out: &mut [u8],
) -> Result<HexDigits, DtoaError> {
    #[cfg(target_arch = "x86_64")]
    {
        let mut frac: u64 = 0;
        let mut i = 0usize;
        while i < 8 {
            frac |= u64::from(v.0[i]) << (i * 8);
            i += 1;
        }
        let se = u16::from(v.0[8]) | (u16::from(v.0[9]) << 8);
        let negative = se & 0x8000 != 0;
        let exp = i32::from(se & 0x7fff);
        if exp == 0x7fff {
            let kind = if frac == 0x8000_0000_0000_0000 {
                Kind::Inf
            } else {
                Kind::Nan
            };
            return Ok(HexDigits {
                len: 0,
                exp2: 0,
                negative,
                kind,
            });
        }
        if exp == 0 {
            if frac == 0 {
                return zero(negative, out);
            }
            if frac >> 63 != 0 {
                return Ok(HexDigits {
                    len: 0,
                    exp2: 0,
                    negative,
                    kind: Kind::Nan,
                });
            }
            let h = 63i32.wrapping_sub(frac.leading_zeros() as i32);
            let sig = u128::from(frac) << (63i32.wrapping_sub(h) as u32);
            let exp2 = h.wrapping_sub(16448);
            return hex_digits(
                sig,
                16,
                exp2,
                negative,
                ndigits,
                rounding,
                upper,
                out,
                Kind::Finite,
            );
        }
        if frac >> 63 == 0 {
            return Ok(HexDigits {
                len: 0,
                exp2: 0,
                negative,
                kind: Kind::Nan,
            });
        }
        hex_digits(
            u128::from(frac),
            16,
            exp.wrapping_sub(16386),
            negative,
            ndigits,
            rounding,
            upper,
            out,
            Kind::Finite,
        )
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        let raw = u128::from_le_bytes(v.0);
        let negative = raw >> 127 != 0;
        let exp = ((raw >> 112) & 0x7fff) as i32;
        let frac = raw & ((1u128 << 112) - 1);
        if exp == 0x7fff {
            let kind = if frac == 0 { Kind::Inf } else { Kind::Nan };
            return Ok(HexDigits {
                len: 0,
                exp2: 0,
                negative,
                kind,
            });
        }
        if exp == 0 {
            if frac == 0 {
                return zero(negative, out);
            }
            let h = 127i32.wrapping_sub(frac.leading_zeros() as i32);
            let sig = frac << (112i32.wrapping_sub(h) as u32);
            let exp2 = h.wrapping_sub(16494);
            return hex_digits(
                sig,
                29,
                exp2,
                negative,
                ndigits,
                rounding,
                upper,
                out,
                Kind::Finite,
            );
        }
        hex_digits(
            (1u128 << 112) | frac,
            29,
            exp.wrapping_sub(16383),
            negative,
            ndigits,
            rounding,
            upper,
            out,
            Kind::Finite,
        )
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    {
        let _ = (v, upper, ndigits, rounding, out);
        Err(DtoaError::NoMemory)
    }
}

#[cfg(test)]
#[path = "hdtoa_ut.rs"]
mod tests;
