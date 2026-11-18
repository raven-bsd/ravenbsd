// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `alarm(2)`: arm the process SIGALRM timer and return the previous remaining seconds.

use core::ffi::c_uint;

/// `alarm`.
#[unsafe(no_mangle)]
pub extern "C" fn alarm(seconds: c_uint) -> c_uint {
    crate::time::alarm::alarm(seconds)
}
