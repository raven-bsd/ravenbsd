// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `umask(2)`.
use core::ffi::c_uint;

/// Set the process file-creation mask and return the previous one.
#[unsafe(no_mangle)]
pub extern "C" fn umask(mask: c_uint) -> c_uint {
    crate::ffi::platform::global().set_umask(mask)
}
