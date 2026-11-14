// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `get_nprocs_conf(3)`.
use core::ffi::c_int;

use crate::personality_common::RealSysInfo;

/// Number of configured CPUs; 1 before PG-21.
#[unsafe(no_mangle)]
pub extern "C" fn get_nprocs_conf() -> c_int {
    raven_libc_personality_linux::sysinfo::get_nprocs_conf(&mut RealSysInfo)
}
