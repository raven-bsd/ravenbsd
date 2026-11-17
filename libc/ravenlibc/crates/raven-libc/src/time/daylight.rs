// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `daylight` data symbol.

use core::ffi::c_int;

/// POSIX `daylight`, non-zero when the zone observes DST.
#[unsafe(no_mangle)]
pub static mut daylight: c_int = 0;

/// Publish the DST flag.
pub(crate) fn publish(value: c_int) {
    // SAFETY: the shell is single-threaded and the C contract allows `tzset` to replace it.
    unsafe {
        daylight = value;
    }
}
