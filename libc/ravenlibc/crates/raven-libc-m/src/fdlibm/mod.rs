// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: module list for the fdlibm/portable transcendental files transpiled from
// rust:library/compiler-builtins/libm/src/math @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452.

//! The exponential, logarithmic, hyperbolic, error and gamma families.

#[allow(unused_imports)]
pub(crate) use crate::support::{Float, f64_from_bits};
#[allow(unused_imports)]
pub(crate) use crate::{fabs, fabsf, floor, floorf, scalbn, scalbnf, sqrt, sqrtf};

mod acosh;
mod acoshf;
mod asinh;
mod asinhf;
mod atanh;
mod atanhf;
mod cosh;
mod coshf;
mod erf;
mod erff;
mod exp;
mod exp2;
mod exp2f;
mod expf;
mod expm1;
mod expm1f;
mod expo2;
mod k_cos;
mod k_cosf;
mod k_expo2;
mod k_expo2f;
mod k_sin;
mod k_sinf;
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
mod sinh;
mod sinhf;
mod tanh;
mod tanhf;

#[allow(unused_imports)]
use self::acosh::acosh;
#[allow(unused_imports)]
use self::acoshf::acoshf;
#[allow(unused_imports)]
use self::asinh::asinh;
#[allow(unused_imports)]
use self::asinhf::asinhf;
#[allow(unused_imports)]
use self::atanh::atanh;
#[allow(unused_imports)]
use self::atanhf::atanhf;
#[allow(unused_imports)]
use self::cosh::cosh;
#[allow(unused_imports)]
use self::coshf::coshf;
#[allow(unused_imports)]
use self::erf::erf;
#[allow(unused_imports)]
use self::erff::erff;
#[allow(unused_imports)]
use self::exp::exp;
#[allow(unused_imports)]
use self::exp2::exp2;
#[allow(unused_imports)]
use self::exp2f::exp2f;
#[allow(unused_imports)]
use self::expf::expf;
#[allow(unused_imports)]
use self::expm1::expm1;
#[allow(unused_imports)]
use self::expm1f::expm1f;
#[allow(unused_imports)]
use self::expo2::expo2;
#[allow(unused_imports)]
use self::k_cos::k_cos;
#[allow(unused_imports)]
use self::k_cosf::k_cosf;
#[allow(unused_imports)]
use self::k_expo2::k_expo2;
#[allow(unused_imports)]
use self::k_expo2f::k_expo2f;
#[allow(unused_imports)]
use self::k_sin::k_sin;
#[allow(unused_imports)]
use self::k_sinf::k_sinf;
#[allow(unused_imports)]
use self::lgamma::lgamma;
#[allow(unused_imports)]
use self::lgamma_r::lgamma_r;
#[allow(unused_imports)]
use self::lgammaf::lgammaf;
#[allow(unused_imports)]
use self::lgammaf_r::lgammaf_r;
#[allow(unused_imports)]
use self::log::log;
#[allow(unused_imports)]
use self::log1p::log1p;
#[allow(unused_imports)]
use self::log1pf::log1pf;
#[allow(unused_imports)]
use self::log2::log2;
#[allow(unused_imports)]
use self::log2f::log2f;
#[allow(unused_imports)]
use self::log10::log10;
#[allow(unused_imports)]
use self::log10f::log10f;
#[allow(unused_imports)]
use self::logf::logf;
#[allow(unused_imports)]
use self::sinh::sinh;
#[allow(unused_imports)]
use self::sinhf::sinhf;
#[allow(unused_imports)]
use self::tanh::tanh;
#[allow(unused_imports)]
use self::tanhf::tanhf;

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

pub(crate) use self::acoshf::acoshf as acoshf_fn;
pub(crate) use self::asinhf::asinhf as asinhf_fn;
pub(crate) use self::atanhf::atanhf as atanhf_fn;
pub(crate) use self::coshf::coshf as coshf_fn;
pub(crate) use self::erff::erff as erff_fn;
pub(crate) use self::exp2f::exp2f as exp2f_fn;
pub(crate) use self::expf::expf as expf_fn;
pub(crate) use self::expm1f::expm1f as expm1f_fn;
pub(crate) use self::lgammaf::lgammaf as lgammaf_fn;
pub(crate) use self::lgammaf_r::lgammaf_r as lgammaf_r_fn;
pub(crate) use self::log1pf::log1pf as log1pf_fn;
pub(crate) use self::log2f::log2f as log2f_fn;
pub(crate) use self::log10f::log10f as log10f_fn;
pub(crate) use self::logf::logf as logf_fn;
pub(crate) use self::sinhf::sinhf as sinhf_fn;
pub(crate) use self::tanhf::tanhf as tanhf_fn;
