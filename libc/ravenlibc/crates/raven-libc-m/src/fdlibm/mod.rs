// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: module list for the fdlibm/portable transcendental files transpiled from
// rust:library/compiler-builtins/libm/src/math @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452.

//! The exponential, logarithmic, hyperbolic, error and gamma families.

#[allow(unused_imports)]
pub(crate) use crate::support::{Float, f64_from_bits};
#[allow(unused_imports)]
pub(crate) use crate::{fabs, fabsf, floor, floorf, modf, modff, scalbn, scalbnf, sqrt, sqrtf};

mod acos;
mod acosf;
mod acosh;
mod acoshf;
mod asin;
mod asinf;
mod asinh;
mod asinhf;
mod atan;
mod atan2;
mod atan2f;
mod atanf;
mod atanh;
mod atanhf;
mod cos;
mod cosf;
mod cosh;
mod coshf;
mod erf;
mod erff;
mod exp;
mod exp10;
mod exp10f;
mod exp2;
mod exp2f;
mod expf;
mod expm1;
mod expm1f;
mod expo2;
mod j0;
mod j0f;
mod j1;
mod j1f;
mod jn;
mod jnf;
mod k_cos;
mod k_cosf;
mod k_expo2;
mod k_expo2f;
mod k_sin;
mod k_sinf;
mod k_tan;
mod k_tanf;
mod lgamma;
mod lgamma_r;
mod lgammaf;
mod lgammaf_r;
mod log;
mod log10;
mod log10f;
mod log1p;
mod log1pf;
mod log2;
mod log2f;
mod logf;
mod modf;
mod modff;
mod pow;
mod powf;
mod rem_pio2;
mod rem_pio2_large;
mod rem_pio2f;
mod sin;
mod sincos;
mod sincosf;
mod sinf;
mod sinh;
mod sinhf;
mod tan;
mod tanf;
mod tanh;
mod tanhf;
mod tgamma;
mod tgammaf;

#[allow(unused_imports)]
pub(crate) use self::acosh::acosh;
#[allow(unused_imports)]
pub(crate) use self::acoshf::acoshf;
#[allow(unused_imports)]
pub(crate) use self::asinh::asinh;
#[allow(unused_imports)]
pub(crate) use self::asinhf::asinhf;
#[allow(unused_imports)]
pub(crate) use self::atanh::atanh;
#[allow(unused_imports)]
pub(crate) use self::atanhf::atanhf;
#[allow(unused_imports)]
pub(crate) use self::cosh::cosh;
#[allow(unused_imports)]
pub(crate) use self::coshf::coshf;
#[allow(unused_imports)]
pub(crate) use self::erf::erf;
#[allow(unused_imports)]
pub(crate) use self::erff::erff;
#[allow(unused_imports)]
pub(crate) use self::exp::exp;
#[allow(unused_imports)]
pub(crate) use self::exp2::exp2;
#[allow(unused_imports)]
pub(crate) use self::exp2f::exp2f;
#[allow(unused_imports)]
pub(crate) use self::expf::expf;
#[allow(unused_imports)]
pub(crate) use self::expm1::expm1;
#[allow(unused_imports)]
pub(crate) use self::expm1f::expm1f;
#[allow(unused_imports)]
pub(crate) use self::expo2::expo2;
#[allow(unused_imports)]
pub(crate) use self::k_cos::k_cos;
#[allow(unused_imports)]
pub(crate) use self::k_cosf::k_cosf;
#[allow(unused_imports)]
pub(crate) use self::k_expo2::k_expo2;
#[allow(unused_imports)]
pub(crate) use self::k_expo2f::k_expo2f;
#[allow(unused_imports)]
pub(crate) use self::k_sin::k_sin;
#[allow(unused_imports)]
pub(crate) use self::k_sinf::k_sinf;
#[allow(unused_imports)]
pub(crate) use self::lgamma::lgamma;
#[allow(unused_imports)]
pub(crate) use self::lgamma_r::lgamma_r;
#[allow(unused_imports)]
pub(crate) use self::lgammaf::lgammaf;
#[allow(unused_imports)]
pub(crate) use self::lgammaf_r::lgammaf_r;
#[allow(unused_imports)]
pub(crate) use self::log::log;
#[allow(unused_imports)]
pub(crate) use self::log1p::log1p;
#[allow(unused_imports)]
pub(crate) use self::log1pf::log1pf;
#[allow(unused_imports)]
pub(crate) use self::log2::log2;
#[allow(unused_imports)]
pub(crate) use self::log2f::log2f;
#[allow(unused_imports)]
pub(crate) use self::log10::log10;
#[allow(unused_imports)]
pub(crate) use self::log10f::log10f;
#[allow(unused_imports)]
pub(crate) use self::logf::logf;
#[allow(unused_imports)]
pub(crate) use self::sinh::sinh;
#[allow(unused_imports)]
pub(crate) use self::sinhf::sinhf;
#[allow(unused_imports)]
pub(crate) use self::tanh::tanh;
#[allow(unused_imports)]
pub(crate) use self::tanhf::tanhf;

/// The high 32 bits of an `f64`.
#[allow(dead_code)]
pub(crate) fn get_high_word(x: f64) -> u32 {
    (x.to_bits() >> 32) as u32
}

/// The low 32 bits of an `f64`.
#[allow(dead_code)]
pub(crate) fn get_low_word(x: f64) -> u32 {
    x.to_bits() as u32
}

/// Replace the high 32 bits of an `f64`.
#[allow(dead_code)]
pub(crate) fn with_set_high_word(f: f64, hi: u32) -> f64 {
    f64::from_bits((u64::from(hi) << 32) | u64::from(get_low_word(f)))
}

/// Replace the low 32 bits of an `f64`.
#[allow(dead_code)]
pub(crate) fn with_set_low_word(f: f64, lo: u32) -> f64 {
    f64::from_bits((f.to_bits() & 0xffff_ffff_0000_0000) | u64::from(lo))
}

/// Combine two 32-bit words into an `f64`.
#[allow(dead_code)]
pub(crate) fn combine_words(hi: u32, lo: u32) -> f64 {
    f64::from_bits((u64::from(hi) << 32) | u64::from(lo))
}

pub(crate) use self::acosh::acosh as acosh_fn;
pub(crate) use self::asinh::asinh as asinh_fn;
pub(crate) use self::atanh::atanh as atanh_fn;
pub(crate) use self::cosh::cosh as cosh_fn;
pub(crate) use self::erf::erf as erf_fn;
pub(crate) use self::erf::erfc as erfc_fn;
pub(crate) use self::exp::exp as exp_fn;
pub(crate) use self::exp2::exp2 as exp2_fn;
pub(crate) use self::expm1::expm1 as expm1_fn;
pub(crate) use self::lgamma::lgamma as lgamma_fn;
pub(crate) use self::lgamma_r::lgamma_r as lgamma_r_fn;
pub(crate) use self::log::log as log_fn;
pub(crate) use self::log1p::log1p as log1p_fn;
pub(crate) use self::log2::log2 as log2_fn;
pub(crate) use self::log10::log10 as log10_fn;
pub(crate) use self::sinh::sinh as sinh_fn;
pub(crate) use self::tanh::tanh as tanh_fn;

pub(crate) use self::acos::acos as acos_fn;
pub(crate) use self::acosf::acosf as acosf_fn;
pub(crate) use self::acoshf::acoshf as acoshf_fn;
pub(crate) use self::asin::asin as asin_fn;
pub(crate) use self::asinf::asinf as asinf_fn;
pub(crate) use self::asinhf::asinhf as asinhf_fn;
pub(crate) use self::atan::atan as atan_fn;
pub(crate) use self::atan2::atan2 as atan2_fn;
pub(crate) use self::atan2f::atan2f as atan2f_fn;
pub(crate) use self::atanf::atanf as atanf_fn;
pub(crate) use self::atanhf::atanhf as atanhf_fn;
pub(crate) use self::cos::cos as cos_fn;
pub(crate) use self::cosf::cosf as cosf_fn;
pub(crate) use self::coshf::coshf as coshf_fn;
pub(crate) use self::erff::erfcf as erfcf_fn;
pub(crate) use self::erff::erff as erff_fn;
pub(crate) use self::exp2f::exp2f as exp2f_fn;
pub(crate) use self::exp10::exp10 as exp10_fn;
pub(crate) use self::exp10f::exp10f as exp10f_fn;
pub(crate) use self::expf::expf as expf_fn;
pub(crate) use self::expm1f::expm1f as expm1f_fn;
pub(crate) use self::j0::j0 as j0_fn;
pub(crate) use self::j0::y0 as y0_fn;
pub(crate) use self::j0f::j0f as j0f_fn;
pub(crate) use self::j0f::y0f as y0f_fn;
pub(crate) use self::j1::j1 as j1_fn;
pub(crate) use self::j1::y1 as y1_fn;
pub(crate) use self::j1f::j1f as j1f_fn;
pub(crate) use self::j1f::y1f as y1f_fn;
pub(crate) use self::jn::jn as jn_fn;
pub(crate) use self::jn::yn as yn_fn;
pub(crate) use self::jnf::jnf as jnf_fn;
pub(crate) use self::jnf::ynf as ynf_fn;
pub(crate) use self::lgammaf::lgammaf as lgammaf_fn;
pub(crate) use self::lgammaf_r::lgammaf_r as lgammaf_r_fn;
pub(crate) use self::log1pf::log1pf as log1pf_fn;
pub(crate) use self::log2f::log2f as log2f_fn;
pub(crate) use self::log10f::log10f as log10f_fn;
pub(crate) use self::logf::logf as logf_fn;
pub(crate) use self::pow::pow as pow_fn;
pub(crate) use self::powf::powf as powf_fn;
pub(crate) use self::sin::sin as sin_fn;
pub(crate) use self::sinf::sinf as sinf_fn;
pub(crate) use self::sinhf::sinhf as sinhf_fn;
pub(crate) use self::tan::tan as tan_fn;
pub(crate) use self::tanf::tanf as tanf_fn;
pub(crate) use self::tanhf::tanhf as tanhf_fn;
pub(crate) use self::tgamma::tgamma as tgamma_fn;
pub(crate) use self::tgammaf::tgammaf as tgammaf_fn;

// The second donor batch: the plain names the sibling files import from `super`.
#[allow(unused_imports)]
pub(crate) use self::acos::acos;
#[allow(unused_imports)]
pub(crate) use self::acosf::acosf;
#[allow(unused_imports)]
pub(crate) use self::asin::asin;
#[allow(unused_imports)]
pub(crate) use self::asinf::asinf;
#[allow(unused_imports)]
pub(crate) use self::atan::atan;
#[allow(unused_imports)]
pub(crate) use self::atan2::atan2;
#[allow(unused_imports)]
pub(crate) use self::atan2f::atan2f;
#[allow(unused_imports)]
pub(crate) use self::atanf::atanf;
#[allow(unused_imports)]
pub(crate) use self::cos::cos;
#[allow(unused_imports)]
pub(crate) use self::cosf::cosf;
#[allow(unused_imports)]
pub(crate) use self::exp10::exp10;
#[allow(unused_imports)]
pub(crate) use self::exp10f::exp10f;
#[allow(unused_imports)]
pub(crate) use self::j0::j0;
#[allow(unused_imports)]
pub(crate) use self::j0::y0;
#[allow(unused_imports)]
pub(crate) use self::j0f::j0f;
#[allow(unused_imports)]
pub(crate) use self::j0f::y0f;
#[allow(unused_imports)]
pub(crate) use self::j1::j1;
#[allow(unused_imports)]
pub(crate) use self::j1::y1;
#[allow(unused_imports)]
pub(crate) use self::j1f::j1f;
#[allow(unused_imports)]
pub(crate) use self::j1f::y1f;
#[allow(unused_imports)]
pub(crate) use self::jn::jn;
#[allow(unused_imports)]
pub(crate) use self::jn::yn;
#[allow(unused_imports)]
pub(crate) use self::jnf::jnf;
#[allow(unused_imports)]
pub(crate) use self::jnf::ynf;
#[allow(unused_imports)]
pub(crate) use self::pow::pow;
#[allow(unused_imports)]
pub(crate) use self::powf::powf;
#[allow(unused_imports)]
pub(crate) use self::sin::sin;
#[allow(unused_imports)]
pub(crate) use self::sinf::sinf;
#[allow(unused_imports)]
pub(crate) use self::tan::tan;
#[allow(unused_imports)]
pub(crate) use self::tanf::tanf;
#[allow(unused_imports)]
pub(crate) use self::tgamma::tgamma;
#[allow(unused_imports)]
pub(crate) use self::tgammaf::tgammaf;

#[allow(unused_imports)]
pub(crate) use self::k_tan::k_tan;
#[allow(unused_imports)]
pub(crate) use self::k_tanf::k_tanf;
#[allow(unused_imports)]
pub(crate) use self::rem_pio2::rem_pio2;
#[allow(unused_imports)]
pub(crate) use self::rem_pio2_large::rem_pio2_large;
#[allow(unused_imports)]
pub(crate) use self::rem_pio2f::rem_pio2f;
#[allow(unused_imports)]
pub(crate) use self::sincos::sincos;
#[allow(unused_imports)]
pub(crate) use self::sincosf::sincosf;
