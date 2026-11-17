// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ftruncate64`.

use super::*;

/// Truncate a descriptor to a 64-bit size.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn ftruncate64(fd: c_int, size: i64) -> c_int {
    unit(raven_libc_platform::compat::ftruncate64(global(), fd, size))
}
