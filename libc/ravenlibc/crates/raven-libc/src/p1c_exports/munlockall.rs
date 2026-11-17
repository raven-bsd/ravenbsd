// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `munlockall`.

use super::*;

/// Unlock all mapped pages.
///
/// # Safety
/// C ABI no arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn munlockall() -> c_int {
    unit(raven_libc_platform::mem::munlockall(global()))
}
