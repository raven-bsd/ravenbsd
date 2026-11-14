// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `signalfd(2)`.
use core::ffi::c_int;

use raven_libc_personality_linux::seams::{Errno, SigSet};

use crate::personality_common::{RealFdTable, RealSignal, ret_int};

/// Create or update a signalfd; `ENOSYS` until PG-7 after argument validation.
///
/// # Safety
/// `mask` points to a readable `sigset_t` (the `signalfd` API requires a non-NULL mask).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn signalfd(fd: c_int, mask: *const SigSet, flags: c_int) -> c_int {
    if mask.is_null() {
        return ret_int(Err(Errno::EFAULT));
    }
    // SAFETY: the caller's sigset_t contract.
    let mask = unsafe { *mask };
    ret_int(raven_libc_personality_linux::signalfd::signalfd(
        &mut RealFdTable,
        &mut RealSignal,
        fd,
        &mask,
        flags,
    ))
}
