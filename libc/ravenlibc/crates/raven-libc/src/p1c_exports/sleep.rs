// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `sleep`.

use super::*;

/// Sleep for whole seconds.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn sleep(seconds: c_uint) -> c_uint {
    raven_libc_platform::time::sleep(global(), seconds).unwrap_or(seconds)
}
