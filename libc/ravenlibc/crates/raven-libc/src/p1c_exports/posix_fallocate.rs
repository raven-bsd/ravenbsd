// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `posix_fallocate`.

use super::*;

/// Allocate file space.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn posix_fallocate(fd: c_int, offset: i64, len: i64) -> c_int {
    unit(global().posix_fallocate(fd, offset, len))
}
