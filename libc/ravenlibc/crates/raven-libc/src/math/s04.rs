// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `yn`.
#[unsafe(no_mangle)]
pub extern "C" fn yn(n: i32, x: f64) -> f64 {
    raven_libc_m::yn(n, x)
}

/// `ynf`.
#[unsafe(no_mangle)]
pub extern "C" fn ynf(n: i32, x: f32) -> f32 {
    raven_libc_m::ynf(n, x)
}

/// `finite`.
#[unsafe(no_mangle)]
pub extern "C" fn finite(x: f64) -> i32 {
    i32::from(raven_libc_m::finite(x))
}

/// `finitef`.
#[unsafe(no_mangle)]
pub extern "C" fn finitef(x: f32) -> i32 {
    i32::from(raven_libc_m::finitef(x))
}
