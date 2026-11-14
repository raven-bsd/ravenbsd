// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `ravenlibc` libm: double and float math, classification and the floating-point environment.
//!
//! The numeric kernels are a provenance-recorded transpile of the vendored Rust `libm` crate
//! (MIT, `rust:library/compiler-builtins/libm`), the ARM optimized-routines and FreeBSD `msun`;
//! the behaviour and header rules are in `architecture/raven-libc/donor-map/05-numerics.md`.
//!
//! This is the exact-class foundation of the crate: `support`, `generic`, `portable`,
//! `coremath` and `fp`. The transcendental, complex, `fenv` and long-double phases are still
//! open and listed in the C47 report.
//!
//! Recursion trap (T-B1..T-B4): no function in this crate calls a `core` float method it
//! implements (`sqrt`, `floor`, `mul_add`, `sin`, ...), and `#![no_builtins]` keeps LLVM from
//! recognising the idioms in our own code.
//!
//! The crate is `no_std`, allocates nothing, and depends only on `raven-libc-abi`.

#![cfg_attr(not(test), no_std)]
#![no_builtins]
#![deny(unsafe_code)]
#![warn(missing_docs)]
#![allow(unexpected_cfgs)] // the donor files carry cfg gates for f16/f128/nightly features that do not exist here
#![allow(
    dead_code,
    unused_imports,
    unused_macros,
    unstable_name_collisions,
    clippy::assign_op_pattern,
    clippy::deprecated_cfg_attr,
    clippy::eq_op,
    clippy::float_cmp,
    clippy::int_plus_one,
    clippy::just_underscores_and_digits,
    clippy::many_single_char_names,
    clippy::manual_div_ceil,
    clippy::mixed_case_hex_literals,
    clippy::needless_late_init,
    clippy::needless_return,
    clippy::unreadable_literal,
    clippy::zero_divided_by_zero
)]
// The transpiled support tree carries helpers and macros that the partial slice does not call yet;
// the donor upstream crate allows the same lint set in its root.
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::excessive_precision,
    clippy::approx_constant
)]
// Numeric kernels index fixed tables with indices bounded by construction and manipulate IEEE
// bit fields with wrapping semantics; the exhaustive tests cover them.

pub mod consts;
pub mod fenv;
pub mod fp;

/// Whether the software floating-point environment is selected.
#[cfg(all(
    target_arch = "x86_64",
    not(feature = "soft-float"),
    target_feature = "sse2"
))]
pub(crate) const SOFT_FLOAT: bool = false;

/// Whether the software floating-point environment is selected.
#[cfg(not(all(
    target_arch = "x86_64",
    not(feature = "soft-float"),
    target_feature = "sse2"
)))]
pub(crate) const SOFT_FLOAT: bool = true;

#[macro_use]
mod macros;
mod rsqrt;
mod sinpi;
#[macro_use]
mod support;
pub mod binary128;
mod c23;
mod complex;
mod coremath;
mod fdlibm;
mod generic;
mod legacy;
#[cfg(target_arch = "x86_64")]
mod long_double;
#[cfg(target_arch = "x86_64")]
pub mod long_double_math;
mod portable;

pub use c23::{
    canonicalize, canonicalizef, getpayload, getpayloadf, issignaling, issignalingf, nextdown,
    nextup, roundeven, setpayload, setpayloadf, setpayloadsig, setpayloadsigf, totalorder,
    totalorderf, totalordermag, totalordermagf,
};
pub use complex::{
    Complex, Real, cabs, cabsf, cacos, cacosf, cacosh, cacoshf, carg, cargf, casin, casinf, casinh,
    casinhf, catan, catanf, catanh, catanhf, ccos, ccosf, ccosh, ccoshf, cexp, cexpf, cimag,
    cimagf, clog, clogf, conj, conjf, cpow, cpowf, cproj, cprojf, creal, crealf, csin, csinf,
    csinh, csinhf, csqrt, csqrtf, ctan, ctanf, ctanh, ctanhf,
};
pub use fenv::{
    Femode, FenvT, FexceptT, feclearexcept, fedisableexcept, feenableexcept, fegetenv, fegetexcept,
    fegetexceptflag, fegetmode, fegetround, feholdexcept, feraiseexcept, fesetenv, fesetexceptflag,
    fesetmode, fesetround, fetestexcept, feupdateenv,
};
pub use fp::{flt_rounds, raise};
pub use fp::{
    fpclassify, fpclassifyf, isfinite, isfinitef, isinf, isinff, isnan, isnanf, isnormal,
    isnormalf, nan, nanf, signbit, signbitf,
};
pub use legacy::{
    drem, dremf, finite, finitef, gamma, gammaf, pow10, pow10f, scalb, scalbf, significand,
    significandf, sincos, sincosf,
};
#[cfg(target_arch = "x86_64")]
pub use long_double::{
    LongDouble, ceill, copysignl, fabsl, fdiml, floorl, fmaxl, fminl, fmodl, fpclassifyl,
    isfinitel, isinfl, isnanl, isnormall, llrintl, llroundl, lrintl, lroundl, nanl, nearbyintl,
    nextafterl, nexttowardl, remainderl, remquol, rintl, roundevenl, roundl, scalbnl, signbitl,
    sqrtl, totalorderl, truncl,
};
#[cfg(target_arch = "x86_64")]
pub use long_double_math::{
    acoshl, acosl, asinhl, asinl, atan2l, atanhl, atanl, coshl, cosl, erfcl, erfl, exp2l, exp10l,
    expl, expm1l, j0l, j1l, jnl, lgamma_rl, lgammal, log1pl, log2l, log10l, logl, powl, sinhl,
    sinl, tanhl, tanl, tgammal, y0l, y1l, ynl,
};

use support::Round;

/// The smallest f64 greater than `x` (C23 `nextup` is not part of this partial slice).
#[inline]
pub fn fabs(x: f64) -> f64 {
    generic::fabs(x)
}

/// The absolute value of an f32.
#[inline]
pub fn fabsf(x: f32) -> f32 {
    generic::fabs(x)
}

/// Copy the sign of `y` onto `x`.
#[inline]
pub fn copysign(x: f64, y: f64) -> f64 {
    generic::copysign(x, y)
}

/// Copy the sign of `y` onto an f32 `x`.
#[inline]
pub fn copysignf(x: f32, y: f32) -> f32 {
    generic::copysign(x, y)
}

/// The largest integer not greater than `x`.
#[inline]
pub fn floor(x: f64) -> f64 {
    generic::floor_status(x).val
}

/// The largest integer not greater than an f32 `x`.
#[inline]
pub fn floorf(x: f32) -> f32 {
    generic::floor_status(x).val
}

/// The smallest integer not less than `x`.
#[inline]
pub fn ceil(x: f64) -> f64 {
    generic::ceil_status(x).val
}

/// The smallest integer not less than an f32 `x`.
#[inline]
pub fn ceilf(x: f32) -> f32 {
    generic::ceil_status(x).val
}

/// `x` rounded to the nearest integer in the current (nearest) mode.
#[inline]
pub fn rint(x: f64) -> f64 {
    generic::rint_status(x).val
}

/// `x` rounded to the nearest integer, f32.
#[inline]
pub fn rintf(x: f32) -> f32 {
    generic::rint_status(x).val
}

/// `x` rounded to the nearest integer, halfway cases away from zero.
#[inline]
pub fn round(x: f64) -> f64 {
    generic::round(x)
}

/// `x` rounded to the nearest integer away from zero, f32.
#[inline]
pub fn roundf(x: f32) -> f32 {
    generic::round(x)
}

/// `x` truncated toward zero.
#[inline]
pub fn trunc(x: f64) -> f64 {
    generic::trunc_status(x).val
}

/// `x` truncated toward zero, f32.
#[inline]
pub fn truncf(x: f32) -> f32 {
    generic::trunc_status(x).val
}

/// The positive difference `max(x - y, +0)`.
#[inline]
pub fn fdim(x: f64, y: f64) -> f64 {
    generic::fdim(x, y)
}

/// `fdim` for f32.
#[inline]
pub fn fdimf(x: f32, y: f32) -> f32 {
    generic::fdim(x, y)
}

/// The larger of `x` and `y`, ignoring a single NaN (IEEE `maxNum`).
#[inline]
pub fn fmax(x: f64, y: f64) -> f64 {
    generic::fmax(x, y)
}

/// `fmax` for f32.
#[inline]
pub fn fmaxf(x: f32, y: f32) -> f32 {
    generic::fmax(x, y)
}

/// The smaller of `x` and `y`, ignoring a single NaN.
#[inline]
pub fn fmin(x: f64, y: f64) -> f64 {
    generic::fmin(x, y)
}

/// `fmin` for f32.
#[inline]
pub fn fminf(x: f32, y: f32) -> f32 {
    generic::fmin(x, y)
}

/// IEEE 754-2019 `maximum`: a NaN operand makes the result NaN.
#[inline]
pub fn fmaximum(x: f64, y: f64) -> f64 {
    generic::fmaximum(x, y)
}

/// `fmaximum` for f32.
#[inline]
pub fn fmaximumf(x: f32, y: f32) -> f32 {
    generic::fmaximum(x, y)
}

/// IEEE 754-2019 `maximumNumber`: a NaN operand is ignored.
#[inline]
pub fn fmaximum_num(x: f64, y: f64) -> f64 {
    generic::fmaximum_num(x, y)
}

/// `fmaximum_num` for f32.
#[inline]
pub fn fmaximum_numf(x: f32, y: f32) -> f32 {
    generic::fmaximum_num(x, y)
}

/// IEEE 754-2019 `minimum`.
#[inline]
pub fn fminimum(x: f64, y: f64) -> f64 {
    generic::fminimum(x, y)
}

/// `fminimum` for f32.
#[inline]
pub fn fminimumf(x: f32, y: f32) -> f32 {
    generic::fminimum(x, y)
}

/// IEEE 754-2019 `minimumNumber`.
#[inline]
pub fn fminimum_num(x: f64, y: f64) -> f64 {
    generic::fminimum_num(x, y)
}

/// `fminimum_num` for f32.
#[inline]
pub fn fminimum_numf(x: f32, y: f32) -> f32 {
    generic::fminimum_num(x, y)
}

/// The floating remainder of `x / y`.
#[inline]
pub fn fmod(x: f64, y: f64) -> f64 {
    generic::fmod(x, y)
}

/// `fmod` for f32.
#[inline]
pub fn fmodf(x: f32, y: f32) -> f32 {
    generic::fmod(x, y)
}

/// Split `x` into a fraction in `[0.5, 1)` and a power of two.
#[inline]
pub fn frexp(x: f64) -> (f64, i32) {
    generic::frexp(x)
}

/// `frexp` for f32.
#[inline]
pub fn frexpf(x: f32) -> (f32, i32) {
    generic::frexp(x)
}

/// The exponent of `x` as an integer.
#[inline]
pub fn ilogb(x: f64) -> i32 {
    generic::ilogb(x)
}

/// `ilogb` for f32.
#[inline]
pub fn ilogbf(x: f32) -> i32 {
    generic::ilogb(x)
}

/// `x * 2^n` computed exactly.
#[inline]
pub fn scalbn(x: f64, n: i32) -> f64 {
    generic::scalbn(x, n)
}

/// `scalbn` for f32.
#[inline]
pub fn scalbnf(x: f32, n: i32) -> f32 {
    generic::scalbn(x, n)
}

/// `x * 2^n` (the `long` exponent form).
#[inline]
pub fn ldexp(x: f64, n: i32) -> f64 {
    portable::ldexp(x, n)
}

/// `ldexp` for f32.
#[inline]
pub fn ldexpf(x: f32, n: i32) -> f32 {
    portable::ldexpf(x, n)
}

/// The correctly rounded square root of `x`.
#[inline]
pub fn sqrt(x: f64) -> f64 {
    generic::sqrt_round(x, Round::Nearest).val
}

/// The correctly rounded square root of an f32.
#[inline]
pub fn sqrtf(x: f32) -> f32 {
    generic::sqrt_round(x, Round::Nearest).val
}

/// Fused multiply-add `x * y + z` with one rounding.
#[inline]
pub fn fma(x: f64, y: f64, z: f64) -> f64 {
    generic::fma_round(x, y, z, Round::Nearest).val
}

/// `fma` for f32.
#[inline]
pub fn fmaf(x: f32, y: f32, z: f32) -> f32 {
    generic::fma_round(x, y, z, Round::Nearest).val
}

/// The cube root of `x`.
#[inline]
pub fn cbrt(x: f64) -> f64 {
    coremath::cbrt(x)
}

/// The cube root of an f32.
#[inline]
pub fn cbrtf(x: f32) -> f32 {
    coremath::cbrtf(x)
}

/// `sqrt(x*x + y*y)` without undue overflow or underflow.
#[inline]
pub fn hypot(x: f64, y: f64) -> f64 {
    coremath::hypot(x, y)
}

/// `hypot` for f32.
#[inline]
pub fn hypotf(x: f32, y: f32) -> f32 {
    coremath::hypotf(x, y)
}

/// The fractional and integral parts of `x`.
#[inline]
pub fn modf(x: f64) -> (f64, f64) {
    portable::modf(x)
}

/// `modf` for f32.
#[inline]
pub fn modff(x: f32) -> (f32, f32) {
    portable::modff(x)
}

/// The exponential of `x`.
#[inline]
pub fn exp(x: f64) -> f64 {
    fdlibm::exp_fn(x)
}

/// The exponential of an f32.
#[inline]
pub fn expf(x: f32) -> f32 {
    fdlibm::expf_fn(x)
}

/// `2` raised to `x`.
#[inline]
pub fn exp2(x: f64) -> f64 {
    fdlibm::exp2_fn(x)
}

/// `2` raised to an f32 `x`.
#[inline]
pub fn exp2f(x: f32) -> f32 {
    fdlibm::exp2f_fn(x)
}

/// `exp(x) - 1`, accurate near zero.
#[inline]
pub fn expm1(x: f64) -> f64 {
    fdlibm::expm1_fn(x)
}

/// `expm1` for f32.
#[inline]
pub fn expm1f(x: f32) -> f32 {
    fdlibm::expm1f_fn(x)
}

/// The natural logarithm of `x`.
#[inline]
pub fn log(x: f64) -> f64 {
    fdlibm::log_fn(x)
}

/// The natural logarithm of an f32.
#[inline]
pub fn logf(x: f32) -> f32 {
    fdlibm::logf_fn(x)
}

/// The base-2 logarithm of `x`.
#[inline]
pub fn log2(x: f64) -> f64 {
    fdlibm::log2_fn(x)
}

/// The base-2 logarithm of an f32.
#[inline]
pub fn log2f(x: f32) -> f32 {
    fdlibm::log2f_fn(x)
}

/// The base-10 logarithm of `x`.
#[inline]
pub fn log10(x: f64) -> f64 {
    fdlibm::log10_fn(x)
}

/// The base-10 logarithm of an f32.
#[inline]
pub fn log10f(x: f32) -> f32 {
    fdlibm::log10f_fn(x)
}

/// `log(1 + x)`, accurate near zero.
#[inline]
pub fn log1p(x: f64) -> f64 {
    fdlibm::log1p_fn(x)
}

/// `log1p` for f32.
#[inline]
pub fn log1pf(x: f32) -> f32 {
    fdlibm::log1pf_fn(x)
}

/// The hyperbolic sine of `x`.
#[inline]
pub fn sinh(x: f64) -> f64 {
    fdlibm::sinh_fn(x)
}

/// The hyperbolic sine of an f32.
#[inline]
pub fn sinhf(x: f32) -> f32 {
    fdlibm::sinhf_fn(x)
}

/// The hyperbolic cosine of `x`.
#[inline]
pub fn cosh(x: f64) -> f64 {
    fdlibm::cosh_fn(x)
}

/// The hyperbolic cosine of an f32.
#[inline]
pub fn coshf(x: f32) -> f32 {
    fdlibm::coshf_fn(x)
}

/// The hyperbolic tangent of `x`.
#[inline]
pub fn tanh(x: f64) -> f64 {
    fdlibm::tanh_fn(x)
}

/// The hyperbolic tangent of an f32.
#[inline]
pub fn tanhf(x: f32) -> f32 {
    fdlibm::tanhf_fn(x)
}

/// The inverse hyperbolic sine of `x`.
#[inline]
pub fn asinh(x: f64) -> f64 {
    fdlibm::asinh_fn(x)
}

/// The inverse hyperbolic sine of an f32.
#[inline]
pub fn asinhf(x: f32) -> f32 {
    fdlibm::asinhf_fn(x)
}

/// The inverse hyperbolic cosine of `x`.
#[inline]
pub fn acosh(x: f64) -> f64 {
    fdlibm::acosh_fn(x)
}

/// The inverse hyperbolic cosine of an f32.
#[inline]
pub fn acoshf(x: f32) -> f32 {
    fdlibm::acoshf_fn(x)
}

/// The inverse hyperbolic tangent of `x`.
#[inline]
pub fn atanh(x: f64) -> f64 {
    fdlibm::atanh_fn(x)
}

/// The inverse hyperbolic tangent of an f32.
#[inline]
pub fn atanhf(x: f32) -> f32 {
    fdlibm::atanhf_fn(x)
}

/// The error function of `x`.
#[inline]
pub fn erf(x: f64) -> f64 {
    fdlibm::erf_fn(x)
}

/// The error function of an f32.
#[inline]
pub fn erff(x: f32) -> f32 {
    fdlibm::erff_fn(x)
}

/// The complementary error function of `x`.
#[inline]
pub fn erfc(x: f64) -> f64 {
    fdlibm::erfc_fn(x)
}

/// The complementary error function of an f32.
#[inline]
pub fn erfcf(x: f32) -> f32 {
    fdlibm::erfcf_fn(x)
}

/// `log|Gamma(x)|` and the sign of `Gamma(x)`.
#[inline]
pub fn lgamma_r(x: f64) -> (f64, i32) {
    fdlibm::lgamma_r_fn(x)
}

/// `lgamma_r` for f32.
#[inline]
pub fn lgammaf_r(x: f32) -> (f32, i32) {
    fdlibm::lgammaf_r_fn(x)
}

/// `log|Gamma(x)|`.
#[inline]
pub fn lgamma(x: f64) -> f64 {
    fdlibm::lgamma_fn(x)
}

/// `lgamma` for f32.
#[inline]
pub fn lgammaf(x: f32) -> f32 {
    fdlibm::lgammaf_fn(x)
}

/// The sine of `x`, in radians.
#[inline]
pub fn sin(x: f64) -> f64 {
    fdlibm::sin_fn(x)
}

/// The sine of an f32.
#[inline]
pub fn sinf(x: f32) -> f32 {
    fdlibm::sinf_fn(x)
}

/// The cosine of `x`, in radians.
#[inline]
pub fn cos(x: f64) -> f64 {
    fdlibm::cos_fn(x)
}

/// The cosine of an f32.
#[inline]
pub fn cosf(x: f32) -> f32 {
    fdlibm::cosf_fn(x)
}

/// The tangent of `x`, in radians.
#[inline]
pub fn tan(x: f64) -> f64 {
    fdlibm::tan_fn(x)
}

/// The tangent of an f32.
#[inline]
pub fn tanf(x: f32) -> f32 {
    fdlibm::tanf_fn(x)
}

/// The arcsine of `x`.
#[inline]
pub fn asin(x: f64) -> f64 {
    fdlibm::asin_fn(x)
}

/// The arcsine of an f32.
#[inline]
pub fn asinf(x: f32) -> f32 {
    fdlibm::asinf_fn(x)
}

/// The arccosine of `x`.
#[inline]
pub fn acos(x: f64) -> f64 {
    fdlibm::acos_fn(x)
}

/// The arccosine of an f32.
#[inline]
pub fn acosf(x: f32) -> f32 {
    fdlibm::acosf_fn(x)
}

/// The arctangent of `x`.
#[inline]
pub fn atan(x: f64) -> f64 {
    fdlibm::atan_fn(x)
}

/// The arctangent of an f32.
#[inline]
pub fn atanf(x: f32) -> f32 {
    fdlibm::atanf_fn(x)
}

/// The arctangent of `y / x`, using the signs to pick the quadrant.
#[inline]
pub fn atan2(y: f64, x: f64) -> f64 {
    fdlibm::atan2_fn(y, x)
}

/// `atan2` for f32.
#[inline]
pub fn atan2f(y: f32, x: f32) -> f32 {
    fdlibm::atan2f_fn(y, x)
}

/// `x` raised to `y`.
#[inline]
pub fn pow(x: f64, y: f64) -> f64 {
    fdlibm::pow_fn(x, y)
}

/// `x` raised to `y`, f32.
#[inline]
pub fn powf(x: f32, y: f32) -> f32 {
    fdlibm::powf_fn(x, y)
}

/// `10` raised to `x`.
#[inline]
pub fn exp10(x: f64) -> f64 {
    fdlibm::exp10_fn(x)
}

/// `10` raised to an f32 `x`.
#[inline]
pub fn exp10f(x: f32) -> f32 {
    fdlibm::exp10f_fn(x)
}

/// The gamma function of `x`.
#[inline]
pub fn tgamma(x: f64) -> f64 {
    fdlibm::tgamma_fn(x)
}

/// The gamma function of an f32.
#[inline]
pub fn tgammaf(x: f32) -> f32 {
    fdlibm::tgammaf_fn(x)
}

/// The Bessel function of the first kind, order zero.
#[inline]
pub fn j0(x: f64) -> f64 {
    fdlibm::j0_fn(x)
}

/// `j0` for f32.
#[inline]
pub fn j0f(x: f32) -> f32 {
    fdlibm::j0f_fn(x)
}

/// The Bessel function of the first kind, order one.
#[inline]
pub fn j1(x: f64) -> f64 {
    fdlibm::j1_fn(x)
}

/// `j1` for f32.
#[inline]
pub fn j1f(x: f32) -> f32 {
    fdlibm::j1f_fn(x)
}

/// The Bessel function of the first kind, order `n`.
#[inline]
pub fn jn(n: i32, x: f64) -> f64 {
    fdlibm::jn_fn(n, x)
}

/// `jn` for f32.
#[inline]
pub fn jnf(n: i32, x: f32) -> f32 {
    fdlibm::jnf_fn(n, x)
}

/// The Bessel function of the second kind, order zero.
#[inline]
pub fn y0(x: f64) -> f64 {
    fdlibm::y0_fn(x)
}

/// `y0` for f32.
#[inline]
pub fn y0f(x: f32) -> f32 {
    fdlibm::y0f_fn(x)
}

/// The Bessel function of the second kind, order one.
#[inline]
pub fn y1(x: f64) -> f64 {
    fdlibm::y1_fn(x)
}

/// `y1` for f32.
#[inline]
pub fn y1f(x: f32) -> f32 {
    fdlibm::y1f_fn(x)
}

/// The Bessel function of the second kind, order `n`.
#[inline]
pub fn yn(n: i32, x: f64) -> f64 {
    fdlibm::yn_fn(n, x)
}

/// `yn` for f32.
#[inline]
pub fn ynf(n: i32, x: f32) -> f32 {
    fdlibm::ynf_fn(n, x)
}

/// The reciprocal square root of `x`.
#[inline]
pub fn rsqrt(x: f64) -> f64 {
    rsqrt::rsqrt(x)
}

/// The reciprocal square root of an f32.
#[inline]
pub fn rsqrtf(x: f32) -> f32 {
    rsqrt::rsqrtf(x)
}

/// `sin(pi * x)`.
#[inline]
pub fn sinpi(x: f64) -> f64 {
    sinpi::sinpi(x)
}

/// `sin(pi * x)` for f32.
#[inline]
pub fn sinpif(x: f32) -> f32 {
    sinpi::sinpif(x)
}

/// C23 `fmaximum_mag`.
#[inline]
pub fn fmaximum_mag(x: f64, y: f64) -> f64 {
    generic::fmaximum_mag(x, y)
}

/// C23 `fmaximum_mag` for f32.
#[inline]
pub fn fmaximum_magf(x: f32, y: f32) -> f32 {
    generic::fmaximum_mag(x, y)
}

/// C23 `fminimum_mag`.
#[inline]
pub fn fminimum_mag(x: f64, y: f64) -> f64 {
    generic::fminimum_mag(x, y)
}

/// C23 `fminimum_mag` for f32.
#[inline]
pub fn fminimum_magf(x: f32, y: f32) -> f32 {
    generic::fminimum_mag(x, y)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
