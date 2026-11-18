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

/// `ldexpl`: the same scaling as `scalbnl` (C `long double` exponent is an `int`).
pub fn ldexpl(x: LongDouble, n: i32) -> LongDouble {
    x.scalbn(n)
}

/// `scalblnl`: `scalbnl` with a C `long` exponent; out-of-range exponents saturate.
pub fn scalblnl(x: LongDouble, n: i64) -> LongDouble {
    x.scalbn(i32::try_from(n).unwrap_or(if n < 0 { i32::MIN } else { i32::MAX }))
}

/// `frexpl`: the mantissa in `[0.5, 1)` plus the exponent.
pub fn frexpl(x: LongDouble) -> (LongDouble, i32) {
    x.frexp()
}

/// `modfl`: the fractional and integral parts.
pub fn modfl(x: LongDouble) -> (LongDouble, LongDouble) {
    x.modf()
}

/// `ilogbl`.
pub fn ilogbl(x: LongDouble) -> i32 {
    x.ilogb()
}

/// `llogbl`: `ilogbl` widened to a C `long`.
pub fn llogbl(x: LongDouble) -> i64 {
    i64::from(x.ilogb())
}

/// `logbl`.
pub fn logbl(x: LongDouble) -> LongDouble {
    x.logb()
}

/// `significandl`: the value scaled to `[1, 2)`.
pub fn significandl(x: LongDouble) -> LongDouble {
    x.significand()
}

/// `cbrtl`: the real cube root.
pub fn cbrtl(x: LongDouble) -> LongDouble {
    x.cbrt()
}

/// `hypotl`: `sqrt(x^2 + y^2)` without intermediate overflow.
pub fn hypotl(x: LongDouble, y: LongDouble) -> LongDouble {
    x.hypot(y)
}

/// `expl`.
pub fn expl(x: LongDouble) -> LongDouble {
    x.exp()
}

/// `logl`: the natural logarithm.
pub fn logl(x: LongDouble) -> LongDouble {
    x.ln()
}

/// `exp2l`.
pub fn exp2l(x: LongDouble) -> LongDouble {
    x.exp2()
}

/// `log2l`.
pub fn log2l(x: LongDouble) -> LongDouble {
    x.log2()
}

/// `log10l`.
pub fn log10l(x: LongDouble) -> LongDouble {
    x.log10()
}

/// `sinl`.
pub fn sinl(x: LongDouble) -> LongDouble {
    x.sin()
}

/// `cosl`.
pub fn cosl(x: LongDouble) -> LongDouble {
    x.cos()
}

/// `tanl`.
pub fn tanl(x: LongDouble) -> LongDouble {
    x.tan()
}

/// `sincosl`: `sin` and `cos` from one argument reduction.
pub fn sincosl(x: LongDouble) -> (LongDouble, LongDouble) {
    x.sincos()
}

/// `asinl`.
pub fn asinl(x: LongDouble) -> LongDouble {
    x.asin()
}

/// `acosl`.
pub fn acosl(x: LongDouble) -> LongDouble {
    x.acos()
}

/// `atanl`.
pub fn atanl(x: LongDouble) -> LongDouble {
    x.atan()
}

/// `atan2l`.
pub fn atan2l(y: LongDouble, x: LongDouble) -> LongDouble {
    y.atan2(x)
}

/// `sinhl`.
pub fn sinhl(x: LongDouble) -> LongDouble {
    x.sinh()
}

/// `coshl`.
pub fn coshl(x: LongDouble) -> LongDouble {
    x.cosh()
}

/// `tanhl`.
pub fn tanhl(x: LongDouble) -> LongDouble {
    x.tanh()
}

/// `asinhl`.
pub fn asinhl(x: LongDouble) -> LongDouble {
    x.asinh()
}

/// `acoshl`.
pub fn acoshl(x: LongDouble) -> LongDouble {
    x.acosh()
}

/// `atanhl`.
pub fn atanhl(x: LongDouble) -> LongDouble {
    x.atanh()
}

/// `erfl`.
pub fn erfl(x: LongDouble) -> LongDouble {
    x.erf()
}

/// `erfcl`.
pub fn erfcl(x: LongDouble) -> LongDouble {
    x.erfc()
}

/// `lgammal`: `ln|Gamma(x)|`.
pub fn lgammal(x: LongDouble) -> LongDouble {
    x.lgamma()
}

/// `lgammal_r`: `ln|Gamma(x)|` and the sign of `Gamma(x)`.
pub fn lgammal_r(x: LongDouble) -> (LongDouble, i32) {
    x.lgamma_r()
}

/// `gammal` (the GNU alias for `tgammal`).
pub fn gammal(x: LongDouble) -> LongDouble {
    x.gamma()
}

/// `tgammal`.
pub fn tgammal(x: LongDouble) -> LongDouble {
    x.gamma()
}

/// `fmal`: fused multiply-add.
pub fn fmal(x: LongDouble, y: LongDouble, z: LongDouble) -> LongDouble {
    x.fma(y, z)
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
