// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `mlockall`.

use super::*;

/// Lock all mapped pages.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn mlockall(flags: c_int) -> c_int {
    unit(raven_libc_platform::mem::mlockall(global(), flags))
}
