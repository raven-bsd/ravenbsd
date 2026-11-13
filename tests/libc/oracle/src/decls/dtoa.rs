// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `dtoa` oracle: NetBSD gdtoa (`strtod`, `strtof`, `strtold`, `__dtoa`, `__gdtoa`, `__ldtoa`,
//! `__hdtoa`, `__hldtoa` and the format packers).
//!
//! `long double` cannot cross a stable-Rust `extern "C"` boundary by value, so `strtold`/`hldtoa`
//! are compared through `__strtopx_D2A`/`__strtopQ_D2A` (which take a pointer) and the C
//! `__hldtoa` through the small SysV thunk in `tests/diff_dtoa.rs`.

#![allow(missing_docs)] // extern declarations mirror the donor prototypes
use core::ffi::{c_char, c_int, c_void};

/// The donor's `struct FPI`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CFpi {
    pub nbits: c_int,
    pub emin: c_int,
    pub emax: c_int,
    pub rounding: c_int,
    pub sudden_underflow: c_int,
}

#[cfg_attr(feature = "oracle-dtoa", link(name = "oracle_dtoa", kind = "dylib"))]
unsafe extern "C" {
    pub fn oracle_strtod(s: *const c_char, se: *mut *mut c_char) -> f64;
    pub fn oracle_strtof(s: *const c_char, se: *mut *mut c_char) -> f32;
    pub fn oracle___dtoa(
        d: f64,
        mode: c_int,
        ndigits: c_int,
        decpt: *mut c_int,
        sign: *mut c_int,
        rve: *mut *mut c_char,
    ) -> *mut c_char;
    pub fn oracle___gdtoa(
        fpi: *const CFpi,
        be: c_int,
        bits: *mut u32,
        kindp: *mut c_int,
        mode: c_int,
        ndigits: c_int,
        decpt: *mut c_int,
        rve: *mut *mut c_char,
    ) -> *mut c_char;
    pub fn oracle___ldtoa(
        ld: *mut c_void,
        mode: c_int,
        ndigits: c_int,
        decpt: *mut c_int,
        sign: *mut c_int,
        rve: *mut *mut c_char,
    ) -> *mut c_char;
    pub fn oracle___hdtoa(
        d: f64,
        xdigs: *const c_char,
        ndigits: c_int,
        decpt: *mut c_int,
        sign: *mut c_int,
        rve: *mut *mut c_char,
    ) -> *mut c_char;
    pub fn oracle___hldtoa();
    pub fn oracle___strtopx_D2A(
        s: *const c_char,
        sp: *mut *mut c_char,
        out: *mut c_void,
        loc: *mut c_void,
    ) -> c_int;
    pub fn oracle___strtopQ_D2A(
        s: *const c_char,
        sp: *mut *mut c_char,
        out: *mut c_void,
        loc: *mut c_void,
    ) -> c_int;
    pub fn oracle___freedtoa(s: *mut c_char);
}
