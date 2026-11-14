// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setgroupent(3)`.
use core::ffi::c_int;

/// Rewind and report success.
#[unsafe(no_mangle)]
pub extern "C" fn setgroupent(_stayopen: c_int) -> c_int {
    crate::grp::setgrent::setgrent();
    1
}
