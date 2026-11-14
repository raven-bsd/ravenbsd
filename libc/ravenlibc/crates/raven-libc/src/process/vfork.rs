// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `vfork(2)`.
use core::ffi::c_int;

/// The BSD spelling of [`crate::process::fork::fork`]; Raven has one fork primitive.
#[unsafe(no_mangle)]
pub extern "C" fn vfork() -> c_int {
    crate::process::fork::fork()
}
