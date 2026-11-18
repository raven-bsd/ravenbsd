// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pause(2)`: suspend the thread until a queued signal arrives.

use core::ffi::c_int;

/// `pause`.
#[unsafe(no_mangle)]
pub extern "C" fn pause() -> c_int {
    crate::time::alarm::pause()
}
