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
}
