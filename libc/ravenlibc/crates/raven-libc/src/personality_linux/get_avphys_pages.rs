// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `get_avphys_pages(3)`.
use core::ffi::c_long;

use crate::personality_common::RealSysInfo;

/// Free physical pages; `ENOSYS` until PG-21.
#[unsafe(no_mangle)]
pub extern "C" fn get_avphys_pages() -> c_long {
    match raven_libc_personality_linux::sysinfo::get_avphys_pages(&mut RealSysInfo) {
        Ok(value) => c_long::try_from(value).unwrap_or(c_long::MAX),
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
