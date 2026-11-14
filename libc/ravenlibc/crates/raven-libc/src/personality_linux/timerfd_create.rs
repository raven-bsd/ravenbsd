// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `timerfd_create(2)`.
use core::ffi::c_int;

use crate::personality_common::{RealFdTable, RealTimer, TIMER_FDS, ret_int};

/// Create a timerfd.
#[unsafe(no_mangle)]
pub extern "C" fn timerfd_create(clock: c_int, flags: c_int) -> c_int {
    ret_int(TIMER_FDS.with(|state| {
        raven_libc_personality_linux::timerfd::create(
            &mut RealFdTable,
            &mut RealTimer,
            state,
            clock,
            flags,
        )
    }))
}
