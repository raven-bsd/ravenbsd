// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `m` oracle: the musl `src/math` originals behind the raven-libc-m transcendentals.
//!
//! The recipe also exports the musl-internal kernels (`__rem_pio2`, `__sin`, `m_cos`, ...) and
//! the `__math_*` helpers, because the build asserts that `exports` equals every defined global;
//! this file declares only the public surface.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::c_int;

#[cfg_attr(
    feature = "oracle-m-core",
    link(name = "oracle_m_core", kind = "dylib")
)]
unsafe extern "C" {
    pub fn oracle_exp(x: f64) -> f64;
    pub fn oracle_expf(x: f32) -> f32;
    pub fn oracle_exp2(x: f64) -> f64;
    pub fn oracle_exp2f(x: f32) -> f32;
    pub fn oracle_exp10(x: f64) -> f64;
    pub fn oracle_exp10f(x: f32) -> f32;
    pub fn oracle_expm1(x: f64) -> f64;
    pub fn oracle_expm1f(x: f32) -> f32;
    pub fn oracle_log(x: f64) -> f64;
    pub fn oracle_logf(x: f32) -> f32;
    pub fn oracle_log2(x: f64) -> f64;
    pub fn oracle_log2f(x: f32) -> f32;
    pub fn oracle_log10(x: f64) -> f64;
    pub fn oracle_log10f(x: f32) -> f32;
    pub fn oracle_log1p(x: f64) -> f64;
    pub fn oracle_log1pf(x: f32) -> f32;
    pub fn oracle_pow(x: f64, y: f64) -> f64;
    pub fn oracle_powf(x: f32, y: f32) -> f32;
    pub fn oracle_sin(x: f64) -> f64;
    pub fn oracle_sinf(x: f32) -> f32;
    pub fn oracle_cos(x: f64) -> f64;
    pub fn oracle_cosf(x: f32) -> f32;
    pub fn oracle_tan(x: f64) -> f64;
    pub fn oracle_tanf(x: f32) -> f32;
    pub fn oracle_asin(x: f64) -> f64;
    pub fn oracle_asinf(x: f32) -> f32;
    pub fn oracle_acos(x: f64) -> f64;
    pub fn oracle_acosf(x: f32) -> f32;
    pub fn oracle_atan(x: f64) -> f64;
    pub fn oracle_atanf(x: f32) -> f32;
    pub fn oracle_atan2(x: f64, y: f64) -> f64;
    pub fn oracle_atan2f(x: f32, y: f32) -> f32;
    pub fn oracle_sinh(x: f64) -> f64;
    pub fn oracle_sinhf(x: f32) -> f32;
    pub fn oracle_cosh(x: f64) -> f64;
    pub fn oracle_coshf(x: f32) -> f32;
    pub fn oracle_tanh(x: f64) -> f64;
    pub fn oracle_tanhf(x: f32) -> f32;
    pub fn oracle_asinh(x: f64) -> f64;
    pub fn oracle_asinhf(x: f32) -> f32;
    pub fn oracle_acosh(x: f64) -> f64;
    pub fn oracle_acoshf(x: f32) -> f32;
    pub fn oracle_atanh(x: f64) -> f64;
    pub fn oracle_atanhf(x: f32) -> f32;
    pub fn oracle_erf(x: f64) -> f64;
    pub fn oracle_erff(x: f32) -> f32;
    pub fn oracle_erfc(x: f64) -> f64;
    pub fn oracle_erfcf(x: f32) -> f32;
    pub fn oracle_j0(x: f64) -> f64;
    pub fn oracle_j0f(x: f32) -> f32;
    pub fn oracle_j1(x: f64) -> f64;
    pub fn oracle_j1f(x: f32) -> f32;
    pub fn oracle_y0(x: f64) -> f64;
    pub fn oracle_y0f(x: f32) -> f32;
    pub fn oracle_y1(x: f64) -> f64;
    pub fn oracle_y1f(x: f32) -> f32;
    pub fn oracle_jn(n: c_int, x: f64) -> f64;
    pub fn oracle_jnf(n: c_int, x: f32) -> f32;
    pub fn oracle_yn(n: c_int, x: f64) -> f64;
    pub fn oracle_ynf(n: c_int, x: f32) -> f32;
}
