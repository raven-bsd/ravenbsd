// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `_Fork(2)`.
use core::ffi::c_int;

/// The C11 spelling of [`crate::process::fork::fork`].
#[unsafe(no_mangle)]
pub extern "C" fn _Fork() -> c_int {
    crate::process::fork::fork()
}
