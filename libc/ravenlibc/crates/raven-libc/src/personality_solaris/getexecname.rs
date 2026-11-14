// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getexecname` (stdlib.h).

use core::ffi::c_char;

use raven_libc_personality_solaris::identity;

use crate::personality_solaris::real::RealAux;

#[unsafe(no_mangle)]
pub extern "C" fn getexecname() -> *const c_char {
    identity::getexecname(&RealAux).map_or(core::ptr::null(), |value| value as *const c_char)
}
