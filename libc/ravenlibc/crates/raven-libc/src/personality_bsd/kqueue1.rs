// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `kqueue1(2)` (NetBSD/OpenBSD spelling).
use core::ffi::c_int;

use crate::personality_common::{RealEvqBsd, RealFdTableBsd, ret_int};

/// Create a kqueue; only `O_CLOEXEC` is accepted.
#[unsafe(no_mangle)]
pub extern "C" fn kqueue1(flags: c_int) -> c_int {
    ret_int(raven_libc_personality_bsd::kqueue::kqueue1(
        &mut RealEvqBsd,
        &mut RealFdTableBsd,
        flags,
    ))
}
