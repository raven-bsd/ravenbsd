// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getpagesize`.

use super::*;

/// Return the page size.
///
/// # Safety
/// C ABI no arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn getpagesize() -> c_int {
    raven_libc_platform::mem::getpagesize()
}
