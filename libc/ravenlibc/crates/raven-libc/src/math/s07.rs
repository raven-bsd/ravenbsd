// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `getpayload` (C `long` result; `long` is 64-bit on every Raven target).
#[unsafe(no_mangle)]
pub extern "C" fn getpayload(x: f64) -> i64 {
    i64::try_from(raven_libc_m::getpayload(x)).unwrap_or(i64::MAX)
}

/// `getpayloadf`.
#[unsafe(no_mangle)]
pub extern "C" fn getpayloadf(x: f32) -> i32 {
    i32::try_from(raven_libc_m::getpayloadf(x)).unwrap_or(i32::MAX)
}

/// `setpayload`.
#[unsafe(no_mangle)]
pub extern "C" fn setpayload(x: f64, payload: i64) -> f64 {
    raven_libc_m::setpayload(x, u64::try_from(payload).unwrap_or(0))
}

/// `setpayloadsig`.
#[unsafe(no_mangle)]
pub extern "C" fn setpayloadsig(x: f64, payload: i64) -> f64 {
    raven_libc_m::setpayloadsig(x, u64::try_from(payload).unwrap_or(0))
}
