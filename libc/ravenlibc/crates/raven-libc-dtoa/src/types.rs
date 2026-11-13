// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/gdtoa.h @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: gdtoa.h,v 1.11 2019/08/01 02:27:43 riastradh Exp $)
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
//! The `gdtoa.h` types: `STRTOG_*` flags and the `FPI` description of a binary format.
//!
//! Transpiled from NetBSD `lib/libc/gdtoa/gdtoa.h` (David M. Gay, Lucent Technologies); see
//! `PROVENANCE.toml`. The `FPI_Round_*` numbering is FLT_ROUNDS 0..3, never the host's `FE_*`.

/// The conversion produced zero.
pub const STRTOG_ZERO: i32 = 0;
/// The conversion produced a normal number.
pub const STRTOG_NORMAL: i32 = 1;
/// The conversion produced a denormal number.
pub const STRTOG_DENORMAL: i32 = 2;
/// The conversion produced an infinity.
pub const STRTOG_INFINITE: i32 = 3;
/// The conversion produced a NaN with the canonical pattern.
pub const STRTOG_NAN: i32 = 4;
/// The conversion produced a NaN with a payload from `nan(...)`.
pub const STRTOG_NANBITS: i32 = 5;
/// The input did not start with a number.
pub const STRTOG_NONUMBER: i32 = 6;
/// Mask of the classification part.
pub const STRTOG_RETMASK: i32 = 7;
/// The value is negative.
pub const STRTOG_NEG: i32 = 0x08;
/// Rounded toward zero (inexact).
pub const STRTOG_INEXLO: i32 = 0x10;
/// Rounded away from zero (inexact).
pub const STRTOG_INEXHI: i32 = 0x20;
/// Either inexact bit.
pub const STRTOG_INEXACT: i32 = 0x30;
/// The result underflowed.
pub const STRTOG_UNDERFLOW: i32 = 0x40;
/// The result overflowed.
pub const STRTOG_OVERFLOW: i32 = 0x80;
/// The arena ran out (NetBSD's addition; mapped to `DtoaError::NoMemory`).
pub const STRTOG_NOMEMORY: i32 = 0x100;

/// Round toward zero (`FPI_Round_zero`, FLT_ROUNDS 0).
pub const FPI_ROUND_ZERO: i32 = 0;
/// Round to nearest, ties to even (`FPI_Round_near`, FLT_ROUNDS 1).
pub const FPI_ROUND_NEAR: i32 = 1;
/// Round toward +infinity (`FPI_Round_up`, FLT_ROUNDS 2).
pub const FPI_ROUND_UP: i32 = 2;
/// Round toward -infinity (`FPI_Round_down`, FLT_ROUNDS 3).
pub const FPI_ROUND_DOWN: i32 = 3;

/// Description of one binary floating-point format, as `struct FPI` in `gdtoa.h`.
#[derive(Clone, Copy)]
pub struct Fpi {
    /// Number of significand bits, including the implicit (or x87 explicit) integer bit.
    pub nbits: i32,
    /// Exponent of the least-significant bit of a denormal.
    pub emin: i32,
    /// Exponent of the least-significant bit of the largest normal.
    pub emax: i32,
    /// `FPI_ROUND_*` value.
    pub rounding: i32,
    /// Whether denormals are flushed to zero.
    pub sudden_underflow: i32,
}

impl Fpi {
    /// The binary32 format of `strtof.c` and `g_ffmt.c`.
    #[must_use]
    pub const fn float() -> Self {
        Self {
            nbits: 24,
            emin: 1 - 127 - 24 + 1,
            emax: 254 - 127 - 24 + 1,
            rounding: FPI_ROUND_NEAR,
            sudden_underflow: 0,
        }
    }

    /// The binary64 format of `strtod.c` and `g_dfmt.c`.
    #[must_use]
    pub const fn double() -> Self {
        Self {
            nbits: 53,
            emin: 1 - 1023 - 53 + 1,
            emax: 2046 - 1023 - 53 + 1,
            rounding: FPI_ROUND_NEAR,
            sudden_underflow: 0,
        }
    }

    /// The x87 80-bit format of `strtorx.c`, `strtopx.c` and `ldtoa.c`.
    #[must_use]
    pub const fn ld80() -> Self {
        Self {
            nbits: 64,
            emin: 1 - 16383 - 64 + 1,
            emax: 32766 - 16383 - 64 + 1,
            rounding: FPI_ROUND_NEAR,
            sudden_underflow: 0,
        }
    }

    /// The IEEE binary128 format of `strtorQ.c`, `strtopQ.c` and `ldtoa.c`.
    #[must_use]
    pub const fn ld128() -> Self {
        Self {
            nbits: 113,
            emin: 1 - 16383 - 113 + 1,
            emax: 32766 - 16383 - 113 + 1,
            rounding: FPI_ROUND_NEAR,
            sudden_underflow: 0,
        }
    }

    /// The same format with a different rounding direction.
    #[must_use]
    pub const fn with_rounding(mut self, rounding: i32) -> Self {
        self.rounding = rounding;
        self
    }
}

#[cfg(test)]
#[path = "types_ut.rs"]
mod tests;
