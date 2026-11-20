// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `read(2)`.
use core::ffi::{c_int, c_void};

use crate::personality_common::{EVENT_FDS, RealEvent, RealFdTable};

/// Read up to `count` bytes into `buffer`.
///
/// # Safety
/// `buffer` has `count` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn read(fd: c_int, buffer: *mut c_void, count: usize) -> isize {
    // SAFETY: the caller's buffer contract; a NULL with zero is the empty slice.
    let slice = unsafe { crate::ffi::cstr::slice_mut(buffer.cast(), count) };
    if crate::personality_common::is_eventfd(fd) {
        return crate::ffi::ret::platform_ssize(EVENT_FDS.with(|state| {
            raven_libc_personality_linux::eventfd::read(
                &RealFdTable,
                &mut RealEvent,
                state,
                fd,
                slice,
            )
        }));
    }
    let result = crate::ffi::platform::global().read(fd, slice);
    if fd == 4 {
        let _ = raven_libc_sys::sys::debug_write(
            alloc::format!(
                "rd4 pid={} ok={} n={} err={}\n",
                crate::process::getpid(),
                result.is_ok(),
                result.as_ref().map_or(0, |n| *n),
                result.as_ref().err().map_or(0, |e| e.0)
            )
            .as_bytes(),
        );
    }
    crate::ffi::ret::platform_ssize(result)
}
