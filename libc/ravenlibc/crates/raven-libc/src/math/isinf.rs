// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `isinf`.
#[unsafe(no_mangle)]
pub extern "C" fn isinf(x: core::ffi::c_double) -> core::ffi::c_int {
    i32::from(raven_libc_m::isinf(x))
}
