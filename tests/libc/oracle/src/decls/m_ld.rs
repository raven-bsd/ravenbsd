// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `m-ld` oracle: the msun `ld80` long-double originals behind the RD-33 hidden-pointer thunk.
//!
//! `long double` is passed by hidden pointer, so the thunk (`oracle_rl_expl`) takes and returns the
//! 16-byte image instead of a by-value argument; this module declares only that surface.

#![allow(missing_docs)] // extern declarations mirror the thunk prototypes
use core::ffi::c_void;

#[cfg_attr(feature = "oracle-m-ld", link(name = "oracle_m_ld", kind = "dylib"))]
unsafe extern "C" {
    pub fn oracle_rl_expl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_expm1l(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_logl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_log10l(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_log1pl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_log2l(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_sinhl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_coshl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_tanhl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_asinhl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_acoshl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_atanhl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_erfl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_erfcl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_lgammal_r(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_tgammal(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_ceill(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_floorl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_rintl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_roundl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_truncl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_copysignl(a: *const c_void, b: *const c_void, output: *mut c_void);
    pub fn oracle_rl_fdiml(a: *const c_void, b: *const c_void, output: *mut c_void);
    pub fn oracle_rl_fmaxl(a: *const c_void, b: *const c_void, output: *mut c_void);
    pub fn oracle_rl_fminl(a: *const c_void, b: *const c_void, output: *mut c_void);
    pub fn oracle_rl_fmodl(a: *const c_void, b: *const c_void, output: *mut c_void);
    pub fn oracle_rl_nextafterl(a: *const c_void, b: *const c_void, output: *mut c_void);
    pub fn oracle_rl_nexttowardl(a: *const c_void, b: *const c_void, output: *mut c_void);
    pub fn oracle_rl_remainderl(a: *const c_void, b: *const c_void, output: *mut c_void);
    pub fn oracle_rl_remquol(a: *const c_void, b: *const c_void, output: *mut c_void);
    pub fn oracle_rl_scalbnl(input: *const c_void, n: i32, output: *mut c_void);
    pub fn oracle_rl_sinl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_cosl(input: *const c_void, output: *mut c_void);
    pub fn oracle_rl_tanl(input: *const c_void, output: *mut c_void);
}
