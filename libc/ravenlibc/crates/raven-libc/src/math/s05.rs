// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `sincos`.
///
/// # Safety
/// `sin_out` and `cos_out` are non-NULL writable doubles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sincos(x: f64, sin_out: *mut f64, cos_out: *mut f64) {
    let (sin, cos) = raven_libc_m::sincos(x);
    // SAFETY: the C contract requires both output pointers.
    unsafe {
        *sin_out = sin;
        *cos_out = cos;
    }
}

/// `sincosf`.
///
/// # Safety
/// `sin_out` and `cos_out` are non-NULL writable floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sincosf(x: f32, sin_out: *mut f32, cos_out: *mut f32) {
    let (sin, cos) = raven_libc_m::sincosf(x);
    // SAFETY: the C contract requires both output pointers.
    unsafe {
        *sin_out = sin;
        *cos_out = cos;
    }
}

/// `totalorder`.
#[unsafe(no_mangle)]
pub extern "C" fn totalorder(x: f64, y: f64) -> i32 {
    i32::from(raven_libc_m::totalorder(x, y))
}

/// `totalorderf`.
#[unsafe(no_mangle)]
pub extern "C" fn totalorderf(x: f32, y: f32) -> i32 {
    i32::from(raven_libc_m::totalorderf(x, y))
}
