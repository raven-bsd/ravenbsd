// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `lgamma_r`.
///
/// # Safety
/// `sign` is NULL or points to a writable `int`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lgamma_r(x: f64, sign: *mut i32) -> f64 {
    let (value, sign_value) = raven_libc_m::lgamma_r(x);
    if !sign.is_null() {
        // SAFETY: the C contract says `sign` is writable when non-NULL.
        unsafe { *sign = sign_value };
    }
    value
}

/// `lgammaf_r`.
///
/// # Safety
/// `sign` is NULL or points to a writable `int`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lgammaf_r(x: f32, sign: *mut i32) -> f32 {
    let (value, sign_value) = raven_libc_m::lgammaf_r(x);
    if !sign.is_null() {
        // SAFETY: the C contract says `sign` is writable when non-NULL.
        unsafe { *sign = sign_value };
    }
    value
}

/// `jn`.
#[unsafe(no_mangle)]
pub extern "C" fn jn(n: i32, x: f64) -> f64 {
    raven_libc_m::jn(n, x)
}

/// `jnf`.
#[unsafe(no_mangle)]
pub extern "C" fn jnf(n: i32, x: f32) -> f32 {
    raven_libc_m::jnf(n, x)
}
