// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `cpow`.
#[unsafe(no_mangle)]
pub extern "C" fn cpow(
    z: raven_libc_m::Complex<f64>,
    w: raven_libc_m::Complex<f64>,
) -> raven_libc_m::Complex<f64> {
    raven_libc_m::cpow(z, w)
}

/// `cpowf`.
#[unsafe(no_mangle)]
pub extern "C" fn cpowf(
    z: raven_libc_m::Complex<f32>,
    w: raven_libc_m::Complex<f32>,
) -> raven_libc_m::Complex<f32> {
    raven_libc_m::cpowf(z, w)
}

/// `cpowl`.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn cpowl(
    z: raven_libc_m::Complex<raven_libc_m::LongDouble>,
    w: raven_libc_m::Complex<raven_libc_m::LongDouble>,
) -> raven_libc_m::Complex<raven_libc_m::LongDouble> {
    raven_libc_m::cpowl(z, w)
}
