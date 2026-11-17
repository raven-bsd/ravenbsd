// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `fdatasync`.

use super::*;

/// Flush file data.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn fdatasync(fd: c_int) -> c_int {
    unit(raven_libc_platform::compat::fdatasync(global(), fd))
}
