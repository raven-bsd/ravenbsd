// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `close_range(2)`.
use core::ffi::c_int;

use crate::personality_common::{RealFdTableBsd, ret_unit};

/// Close or mark descriptors in `lo..=hi`.
#[unsafe(no_mangle)]
pub extern "C" fn close_range(lo: u32, hi: u32, flags: u32) -> c_int {
    ret_unit(raven_libc_personality_bsd::closefrom::close_range(
        &mut RealFdTableBsd,
        lo,
        hi,
        flags,
    ))
}
