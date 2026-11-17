// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `ctanh`.
#[unsafe(no_mangle)]
pub extern "C" fn ctanh(z: raven_libc_m::Complex<f64>) -> raven_libc_m::Complex<f64> {
    raven_libc_m::ctanh(z)
}

/// `ctanhf`.
#[unsafe(no_mangle)]
pub extern "C" fn ctanhf(z: raven_libc_m::Complex<f32>) -> raven_libc_m::Complex<f32> {
    raven_libc_m::ctanhf(z)
}

/// `ctanhl`.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn ctanhl(
    z: raven_libc_m::Complex<raven_libc_m::LongDouble>,
) -> raven_libc_m::Complex<raven_libc_m::LongDouble> {
    raven_libc_m::ctanhl(z)
}
