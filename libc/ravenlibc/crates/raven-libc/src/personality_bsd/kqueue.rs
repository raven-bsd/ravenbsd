// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `kqueue(2)`.
use core::ffi::c_int;

use crate::personality_common::{RealEvqBsd, RealFdTableBsd, ret_int};

/// Create a kqueue.
#[unsafe(no_mangle)]
pub extern "C" fn kqueue() -> c_int {
    ret_int(raven_libc_personality_bsd::kqueue::kqueue(
        &mut RealEvqBsd,
        &mut RealFdTableBsd,
        false,
    ))
}
