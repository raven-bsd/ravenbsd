// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `csqrt`.
#[unsafe(no_mangle)]
pub extern "C" fn csqrt(z: raven_libc_m::Complex<f64>) -> raven_libc_m::Complex<f64> {
    raven_libc_m::csqrt(z)
}

/// `csqrtf`.
#[unsafe(no_mangle)]
pub extern "C" fn csqrtf(z: raven_libc_m::Complex<f32>) -> raven_libc_m::Complex<f32> {
    raven_libc_m::csqrtf(z)
}

/// `csqrtl`.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn csqrtl(
    z: raven_libc_m::Complex<raven_libc_m::LongDouble>,
) -> raven_libc_m::Complex<raven_libc_m::LongDouble> {
    raven_libc_m::csqrtl(z)
}
