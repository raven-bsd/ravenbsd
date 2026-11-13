// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `div(3)`, `ldiv(3)`, `lldiv(3)` and `imaxdiv(3)`.
use core::ffi::c_int;

/// The `div_t` record.
#[repr(C)]
pub struct DivT {
    /// Quotient.
    pub quot: c_int,
    /// Remainder.
    pub rem: c_int,
}

/// The `ldiv_t` record (also `lldiv_t`/`imaxdiv_t` on LP64).
#[repr(C)]
pub struct LDivT {
    /// Quotient.
    pub quot: i64,
    /// Remainder.
    pub rem: i64,
}

/// Integer division with remainder.
#[unsafe(no_mangle)]
pub extern "C" fn div(numerator: c_int, denominator: c_int) -> DivT {
    let value = raven_libc_stdlib::arith::div::div(numerator, denominator);
    DivT {
        quot: value.quot,
        rem: value.rem,
    }
}

/// `long` division with remainder.
#[unsafe(no_mangle)]
pub extern "C" fn ldiv(numerator: i64, denominator: i64) -> LDivT {
    let value = raven_libc_stdlib::arith::ldiv::ldiv(numerator, denominator);
    LDivT {
        quot: value.quot,
        rem: value.rem,
    }
}

/// `long long` division with remainder.
#[unsafe(no_mangle)]
pub extern "C" fn lldiv(numerator: i64, denominator: i64) -> LDivT {
    let value = raven_libc_stdlib::arith::lldiv(numerator, denominator);
    LDivT {
        quot: value.quot,
        rem: value.rem,
    }
}

/// `intmax_t` division with remainder.
#[unsafe(no_mangle)]
pub extern "C" fn imaxdiv(numerator: i64, denominator: i64) -> LDivT {
    let value = raven_libc_stdlib::arith::imaxdiv(numerator, denominator);
    LDivT {
        quot: value.quot,
        rem: value.rem,
    }
}
