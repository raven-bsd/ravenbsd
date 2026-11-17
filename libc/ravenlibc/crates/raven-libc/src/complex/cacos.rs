// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `cacos`.
#[unsafe(no_mangle)]
pub extern "C" fn cacos(z: raven_libc_m::Complex<f64>) -> raven_libc_m::Complex<f64> {
    raven_libc_m::cacos(z)
}

/// `cacosf`.
#[unsafe(no_mangle)]
pub extern "C" fn cacosf(z: raven_libc_m::Complex<f32>) -> raven_libc_m::Complex<f32> {
    raven_libc_m::cacosf(z)
}

/// `cacosl`.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn cacosl(
    z: raven_libc_m::Complex<raven_libc_m::LongDouble>,
) -> raven_libc_m::Complex<raven_libc_m::LongDouble> {
    raven_libc_m::cacosl(z)
}
