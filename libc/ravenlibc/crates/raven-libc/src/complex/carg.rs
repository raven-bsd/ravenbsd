// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `carg`.
#[unsafe(no_mangle)]
pub extern "C" fn carg(z: raven_libc_m::Complex<f64>) -> f64 {
    raven_libc_m::carg(z)
}

/// `cargf`.
#[unsafe(no_mangle)]
pub extern "C" fn cargf(z: raven_libc_m::Complex<f32>) -> f32 {
    raven_libc_m::cargf(z)
}

/// `cargl`.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn cargl(
    z: raven_libc_m::Complex<raven_libc_m::LongDouble>,
) -> raven_libc_m::LongDouble {
    raven_libc_m::cargl(z)
}
