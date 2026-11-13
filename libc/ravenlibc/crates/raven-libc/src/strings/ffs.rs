// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ffs(3)`, `ffsl(3)` and `ffsll(3)`.
use core::ffi::c_int;

/// The position of the first set bit, 1-based; 0 when `x` is zero.
#[unsafe(no_mangle)]
pub extern "C" fn ffs(x: c_int) -> c_int {
    raven_libc_str::bits::ffs(x)
}

/// The `long` form.
#[unsafe(no_mangle)]
pub extern "C" fn ffsl(x: i64) -> c_int {
    raven_libc_str::bits::ffsl(x)
}

/// The `long long` form.
#[unsafe(no_mangle)]
pub extern "C" fn ffsll(x: i64) -> c_int {
    raven_libc_str::bits::ffsll(x)
}
