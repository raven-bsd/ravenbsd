// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `*l` entry points for the binary128 `long double` (aarch64/riscv64).
//!
//! The names match the x86-64 ld80 module exactly; each one is a thin call into `LongDouble128`
//! (which is the crate's `LongDouble` on these targets).

use crate::LongDouble;

/// `fabsl`.
pub fn fabsl(x: LongDouble) -> LongDouble {
    x.abs()
}

/// `copysignl`.
pub fn copysignl(x: LongDouble, sign: LongDouble) -> LongDouble {
    x.copysign(sign)
}

/// `sqrtl`.
pub fn sqrtl(x: LongDouble) -> LongDouble {
    x.sqrt()
}

/// `floorl`.
pub fn floorl(x: LongDouble) -> LongDouble {
    x.floor()
}

/// `ceill`.
pub fn ceill(x: LongDouble) -> LongDouble {
    x.ceil()
}

/// `truncl`.
pub fn truncl(x: LongDouble) -> LongDouble {
    x.trunc()
}

/// `roundl`.
pub fn roundl(x: LongDouble) -> LongDouble {
    x.round()
}

/// `roundevenl`.
pub fn roundevenl(x: LongDouble) -> LongDouble {
    x.roundeven()
}

/// `rintl`.
pub fn rintl(x: LongDouble) -> LongDouble {
    x.roundeven()
}

/// `nearbyintl`.
pub fn nearbyintl(x: LongDouble) -> LongDouble {
    x.roundeven()
}

/// `lrintl`.
pub fn lrintl(x: LongDouble) -> i64 {
    x.roundeven().to_i64().unwrap_or(i64::MIN)
}

/// `llrintl`.
pub fn llrintl(x: LongDouble) -> i64 {
    x.roundeven().to_i64().unwrap_or(i64::MIN)
}

/// `lroundl`.
pub fn lroundl(x: LongDouble) -> i64 {
    x.round().to_i64().unwrap_or(i64::MIN)
}

/// `llroundl`.
pub fn llroundl(x: LongDouble) -> i64 {
    x.round().to_i64().unwrap_or(i64::MIN)
}

/// `fmodl`.
pub fn fmodl(x: LongDouble, y: LongDouble) -> LongDouble {
    x.fmod(y)
}

/// `remainderl`.
pub fn remainderl(x: LongDouble, y: LongDouble) -> LongDouble {
    x.remainder(y)
}

/// `remquol`.
pub fn remquol(x: LongDouble, y: LongDouble) -> (LongDouble, i32) {
    x.remquo(y)
}

/// `fdiml`.
pub fn fdiml(x: LongDouble, y: LongDouble) -> LongDouble {
    x.fdim(y)
}

/// `fmaxl`.
pub fn fmaxl(x: LongDouble, y: LongDouble) -> LongDouble {
    x.fmax(y)
}

/// `fminl`.
pub fn fminl(x: LongDouble, y: LongDouble) -> LongDouble {
    x.fmin(y)
}

/// `nextafterl`.
pub fn nextafterl(x: LongDouble, y: LongDouble) -> LongDouble {
    x.next_after(y)
}

/// `nexttowardl`.
pub fn nexttowardl(x: LongDouble, y: LongDouble) -> LongDouble {
    x.next_after(y)
}

/// `scalbnl`.
pub fn scalbnl(x: LongDouble, n: i32) -> LongDouble {
    x.scalbn(n)
}

/// `nanl`.
pub fn nanl() -> LongDouble {
    LongDouble::nan()
}

/// `totalorderl`.
pub fn totalorderl(x: LongDouble, y: LongDouble) -> bool {
    x.total_cmp(y) != core::cmp::Ordering::Greater
}

/// `isnanl`.
pub fn isnanl(x: LongDouble) -> bool {
    x.is_nan()
}

/// `isfinitel`.
pub fn isfinitel(x: LongDouble) -> bool {
    x.is_finite()
}

/// `isinfl`.
pub fn isinfl(x: LongDouble) -> bool {
    x.is_infinite()
}

/// `isnormall`.
pub fn isnormall(x: LongDouble) -> bool {
    x.is_normal()
}

/// `signbitl`.
pub fn signbitl(x: LongDouble) -> bool {
    x.sign_bit()
}

/// `fpclassifyl`.
pub fn fpclassifyl(x: LongDouble) -> i32 {
    x.class()
}
