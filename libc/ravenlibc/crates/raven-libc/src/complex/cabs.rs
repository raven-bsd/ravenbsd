// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `cabs`.
#[unsafe(no_mangle)]
pub extern "C" fn cabs(z: raven_libc_m::Complex<f64>) -> f64 {
    raven_libc_m::cabs(z)
}

/// `cabsf`.
#[unsafe(no_mangle)]
pub extern "C" fn cabsf(z: raven_libc_m::Complex<f32>) -> f32 {
    raven_libc_m::cabsf(z)
}

/// `cabsl`.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn cabsl(
    z: raven_libc_m::Complex<raven_libc_m::LongDouble>,
) -> raven_libc_m::LongDouble {
    raven_libc_m::cabsl(z)
}
