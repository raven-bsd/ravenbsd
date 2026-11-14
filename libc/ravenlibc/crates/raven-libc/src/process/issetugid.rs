// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `issetugid(2)`.
use core::ffi::c_int;

/// Raven never runs a privileged image (H-7): always false.
#[unsafe(no_mangle)]
pub extern "C" fn issetugid() -> c_int {
    c_int::from(raven_libc_proc::ids::issetugid())
}
