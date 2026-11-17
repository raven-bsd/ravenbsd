// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `ccos`.
#[unsafe(no_mangle)]
pub extern "C" fn ccos(z: raven_libc_m::Complex<f64>) -> raven_libc_m::Complex<f64> {
    raven_libc_m::ccos(z)
}

/// `ccosf`.
#[unsafe(no_mangle)]
pub extern "C" fn ccosf(z: raven_libc_m::Complex<f32>) -> raven_libc_m::Complex<f32> {
    raven_libc_m::ccosf(z)
}

/// `ccosl`.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn ccosl(
    z: raven_libc_m::Complex<raven_libc_m::LongDouble>,
) -> raven_libc_m::Complex<raven_libc_m::LongDouble> {
    raven_libc_m::ccosl(z)
}
