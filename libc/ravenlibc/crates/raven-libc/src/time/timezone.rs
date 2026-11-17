// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `timezone` data symbol.

/// POSIX `timezone`, seconds west of UTC.
#[unsafe(no_mangle)]
pub static mut timezone: i64 = 0;

/// Publish the standard offset.
pub(crate) fn publish(value: i64) {
    // SAFETY: the shell is single-threaded and the C contract allows `tzset` to replace it.
    unsafe {
        timezone = value;
    }
}
