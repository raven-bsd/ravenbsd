// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `lockf`.

use super::*;

/// Lock a file range.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn lockf(fd: c_int, function: c_int, size: i64) -> c_int {
    unit(global().lockf(fd, function, size))
}
