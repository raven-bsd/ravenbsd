// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `usleep`.

use super::*;

/// Sleep for microseconds.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn usleep(microseconds: c_uint) -> c_int {
    unit(raven_libc_platform::time::usleep(global(), microseconds))
}
