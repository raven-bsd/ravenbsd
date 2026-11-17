// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `conj`.
#[unsafe(no_mangle)]
pub extern "C" fn conj(z: raven_libc_m::Complex<f64>) -> raven_libc_m::Complex<f64> {
    raven_libc_m::conj(z)
}

/// `conjf`.
#[unsafe(no_mangle)]
pub extern "C" fn conjf(z: raven_libc_m::Complex<f32>) -> raven_libc_m::Complex<f32> {
    raven_libc_m::conjf(z)
}

/// `conjl`.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn conjl(
    z: raven_libc_m::Complex<raven_libc_m::LongDouble>,
) -> raven_libc_m::Complex<raven_libc_m::LongDouble> {
    raven_libc_m::conjl(z)
}
