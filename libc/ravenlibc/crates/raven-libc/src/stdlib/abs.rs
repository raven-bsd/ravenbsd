// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `abs(3)`, `labs(3)`, `llabs(3)` and `imaxabs(3)`.
use core::ffi::c_int;

/// The absolute value of an `int`.
#[unsafe(no_mangle)]
pub extern "C" fn abs(value: c_int) -> c_int {
    raven_libc_stdlib::arith::abs::abs(value)
}

/// The absolute value of a `long`.
#[unsafe(no_mangle)]
pub extern "C" fn labs(value: i64) -> i64 {
    raven_libc_stdlib::arith::labs::labs(value)
}

/// The absolute value of a `long long`.
#[unsafe(no_mangle)]
pub extern "C" fn llabs(value: i64) -> i64 {
    raven_libc_stdlib::arith::llabs(value)
}

/// The absolute value of an `intmax_t`.
#[unsafe(no_mangle)]
pub extern "C" fn imaxabs(value: i64) -> i64 {
    raven_libc_stdlib::arith::imaxabs(value)
}
