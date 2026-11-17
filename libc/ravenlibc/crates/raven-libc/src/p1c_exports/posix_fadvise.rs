// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `posix_fadvise`.

use super::*;

/// Advise the kernel about a file range.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn posix_fadvise(fd: c_int, offset: i64, len: i64, advice: c_int) -> c_int {
    unit(global().posix_fadvise(fd, offset, len, advice))
}
