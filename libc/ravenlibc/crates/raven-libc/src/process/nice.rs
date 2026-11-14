// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `nice(3)`.
use core::ffi::c_int;

/// Adjust the nice value through the C53 increment model.
#[unsafe(no_mangle)]
pub extern "C" fn nice(increment: c_int) -> c_int {
    let result = crate::ffi::map_proc::nice(&crate::ffi::procbackend::backend(), increment);
    crate::ffi::ret::int(result.map_err(crate::ffi::map_proc::error))
}
