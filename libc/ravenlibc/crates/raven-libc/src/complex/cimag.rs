// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `cimag`.
#[unsafe(no_mangle)]
pub extern "C" fn cimag(z: raven_libc_m::Complex<f64>) -> f64 {
    raven_libc_m::cimag(z)
}

/// `cimagf`.
#[unsafe(no_mangle)]
pub extern "C" fn cimagf(z: raven_libc_m::Complex<f32>) -> f32 {
    raven_libc_m::cimagf(z)
}

/// `cimagl`.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn cimagl(
    z: raven_libc_m::Complex<raven_libc_m::LongDouble>,
) -> raven_libc_m::LongDouble {
    raven_libc_m::cimagl(z)
}
