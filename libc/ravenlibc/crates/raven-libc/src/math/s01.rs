// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `fma`.
#[unsafe(no_mangle)]
pub extern "C" fn fma(x: f64, y: f64, z: f64) -> f64 {
    raven_libc_m::fma(x, y, z)
}

/// `fmaf`.
#[unsafe(no_mangle)]
pub extern "C" fn fmaf(x: f32, y: f32, z: f32) -> f32 {
    raven_libc_m::fmaf(x, y, z)
}

/// `ilogb`.
#[unsafe(no_mangle)]
pub extern "C" fn ilogb(x: f64) -> i32 {
    raven_libc_m::ilogb(x)
}

/// `ilogbf`.
#[unsafe(no_mangle)]
pub extern "C" fn ilogbf(x: f32) -> i32 {
    raven_libc_m::ilogbf(x)
}
