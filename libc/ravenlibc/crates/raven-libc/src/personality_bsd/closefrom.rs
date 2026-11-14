// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `closefrom(3)`.
use core::ffi::c_int;

use crate::personality_common::RealFdTableBsd;

/// Close every descriptor at or above `lo`.
#[unsafe(no_mangle)]
pub extern "C" fn closefrom(lo: c_int) {
    raven_libc_personality_bsd::closefrom::closefrom(&mut RealFdTableBsd, lo);
}
