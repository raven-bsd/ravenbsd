// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `clearenv(3)`.
use core::ffi::c_int;

/// Remove every environment entry.
#[unsafe(no_mangle)]
pub extern "C" fn clearenv() -> c_int {
    crate::ffi::ret::int(crate::stdlib::env_support::clear().map(|()| 0))
}
