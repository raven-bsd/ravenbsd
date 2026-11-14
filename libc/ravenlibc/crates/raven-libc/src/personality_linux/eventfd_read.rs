// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `eventfd_read(3)`.
use core::ffi::c_int;

use crate::personality_common::{EVENT_FDS, RealEvent, RealFdTable, ret_int};

/// Read the 64-bit counter.
///
/// # Safety
/// `value` points to one writable `eventfd_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn eventfd_read(fd: c_int, value: *mut u64) -> c_int {
    if value.is_null() {
        return ret_int(Err(raven_libc_platform::Errno::EFAULT));
    }
    // SAFETY: the caller's value contract.
    let out = unsafe { core::slice::from_raw_parts_mut(value.cast::<u8>(), 8) };
    let result = EVENT_FDS.with(|state| {
        raven_libc_personality_linux::eventfd::read(&RealFdTable, &mut RealEvent, state, fd, out)
    });
    match result {
        Ok(_) => 0,
        Err(error) => crate::personality_common::ret_unit(Err(error)),
    }
}
