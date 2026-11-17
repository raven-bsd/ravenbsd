// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `dup3`.

use super::*;

/// Duplicate a descriptor with flags.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn dup3(old: c_int, new: c_int, flags: c_int) -> c_int {
    int(raven_libc_platform::compat::dup3(global(), old, new, flags))
}
