// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ualarm(3)`: arm the process SIGALRM timer with a repeat interval.

use core::ffi::c_uint;

/// `ualarm`.
#[unsafe(no_mangle)]
pub extern "C" fn ualarm(usec: c_uint, interval_usec: c_uint) -> c_uint {
    crate::time::alarm::ualarm(usec, interval_usec)
}
