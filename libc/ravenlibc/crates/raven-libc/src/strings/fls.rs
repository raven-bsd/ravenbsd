// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fls(3)`, `flsl(3)` and `flsll(3)`.
use core::ffi::c_int;

/// The position of the last set bit, 1-based; 0 when `x` is zero.
#[unsafe(no_mangle)]
pub extern "C" fn fls(x: c_int) -> c_int {
    raven_libc_str::bits::fls(x)
}

/// The `long` form.
#[unsafe(no_mangle)]
pub extern "C" fn flsl(x: i64) -> c_int {
    raven_libc_str::bits::flsl(x)
}

/// The `long long` form.
#[unsafe(no_mangle)]
pub extern "C" fn flsll(x: i64) -> c_int {
    raven_libc_str::bits::flsll(x)
}
