// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `frexp`.
///
/// # Safety
/// `exp` is NULL or points to a writable `int`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn frexp(x: f64, exp: *mut i32) -> f64 {
    let (value, exponent) = raven_libc_m::frexp(x);
    if !exp.is_null() {
        // SAFETY: the C contract says `exp` is writable when non-NULL.
        unsafe { *exp = exponent };
    }
    value
}

/// `frexpf`.
///
/// # Safety
/// `exp` is NULL or points to a writable `int`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn frexpf(x: f32, exp: *mut i32) -> f32 {
    let (value, exponent) = raven_libc_m::frexpf(x);
    if !exp.is_null() {
        // SAFETY: the C contract says `exp` is writable when non-NULL.
        unsafe { *exp = exponent };
    }
    value
}

/// `modf`.
///
/// # Safety
/// `iptr` is NULL or points to a writable `double`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn modf(x: f64, iptr: *mut f64) -> f64 {
    let (frac, integer) = raven_libc_m::modf(x);
    if !iptr.is_null() {
        // SAFETY: the C contract says `iptr` is writable when non-NULL.
        unsafe { *iptr = integer };
    }
    frac
}

/// `modff`.
///
/// # Safety
/// `iptr` is NULL or points to a writable `float`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn modff(x: f32, iptr: *mut f32) -> f32 {
    let (frac, integer) = raven_libc_m::modff(x);
    if !iptr.is_null() {
        // SAFETY: the C contract says `iptr` is writable when non-NULL.
        unsafe { *iptr = integer };
    }
    frac
}
