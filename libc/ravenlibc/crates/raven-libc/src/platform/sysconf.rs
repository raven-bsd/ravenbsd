// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sysconf(3)`.
use core::ffi::{c_int, c_long};

/// Return the configured value of `name`.
#[unsafe(no_mangle)]
pub extern "C" fn sysconf(name: c_int) -> c_long {
    match crate::ffi::platform::global().sysconf(name) {
        Ok(value) => value,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
