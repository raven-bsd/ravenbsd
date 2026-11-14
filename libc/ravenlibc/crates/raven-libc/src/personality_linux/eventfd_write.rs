// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `eventfd_write(3)`.
use core::ffi::c_int;

use crate::personality_common::{EVENT_FDS, RealEvent, RealFdTable, ret_int};

/// Add `value` to the counter.
#[unsafe(no_mangle)]
pub extern "C" fn eventfd_write(fd: c_int, value: u64) -> c_int {
    ret_int(EVENT_FDS.with(|state| {
        raven_libc_personality_linux::eventfd::write(&RealFdTable, &mut RealEvent, state, fd, value)
            .map(|()| 0)
    }))
}
