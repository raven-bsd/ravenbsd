// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `_Exit(3)`.
use core::ffi::c_int;

use crate::unistd as io_core;

/// Terminate without running any handler.
#[unsafe(no_mangle)]
pub extern "C" fn _Exit(status: c_int) -> ! {
    io_core::_exit::_exit(status)
}
