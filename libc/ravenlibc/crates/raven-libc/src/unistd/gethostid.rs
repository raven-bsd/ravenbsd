// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `gethostid(2)`.

use core::ffi::c_long;

/// Return the host identifier from `sys:kern/hostid`.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub extern "C" fn gethostid() -> c_long {
    c_long::from(crate::ffi::platform::global().gethostid())
}
