// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `syncfs`.

use super::*;

/// Synchronize a file system.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn syncfs(fd: c_int) -> c_int {
    unit(raven_libc_platform::compat::syncfs(global(), fd))
}
