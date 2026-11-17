// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `creal`.
#[unsafe(no_mangle)]
pub extern "C" fn creal(z: raven_libc_m::Complex<f64>) -> f64 {
    raven_libc_m::creal(z)
}

/// `crealf`.
#[unsafe(no_mangle)]
pub extern "C" fn crealf(z: raven_libc_m::Complex<f32>) -> f32 {
    raven_libc_m::crealf(z)
}

/// `creall`.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn creall(
    z: raven_libc_m::Complex<raven_libc_m::LongDouble>,
) -> raven_libc_m::LongDouble {
    raven_libc_m::creall(z)
}
