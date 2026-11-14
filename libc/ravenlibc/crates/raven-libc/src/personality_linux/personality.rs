// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `personality(2)`.
use core::ffi::c_int;

use crate::personality_common::ret_i64;

/// Query or set the personality; only `PER_LINUX` is accepted.
#[unsafe(no_mangle)]
pub extern "C" fn personality(persona: core::ffi::c_ulong) -> c_int {
    ret_i64(raven_libc_personality_linux::personality::personality(
        persona,
    ))
}
