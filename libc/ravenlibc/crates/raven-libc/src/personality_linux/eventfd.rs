// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `eventfd(2)`.
use core::ffi::{c_int, c_uint};

use crate::personality_common::{EVENT_FDS, RealEvent, RealFdTable, ret_int};

/// Create an eventfd counter.
#[unsafe(no_mangle)]
pub extern "C" fn eventfd(initval: c_uint, flags: c_int) -> c_int {
    ret_int(EVENT_FDS.with(|state| {
        raven_libc_personality_linux::eventfd::create(
            &mut RealFdTable,
            &mut RealEvent,
            state,
            initval,
            flags,
        )
    }))
}
