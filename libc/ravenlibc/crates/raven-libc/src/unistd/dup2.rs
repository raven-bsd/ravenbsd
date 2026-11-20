// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `dup2(2)`.
use core::ffi::c_int;

/// Duplicate `old` onto `new`.
#[unsafe(no_mangle)]
pub extern "C" fn dup2(old: c_int, new: c_int) -> c_int {
    let result = crate::ffi::platform::global().dup2(old, new, false);
    if new == 4 {
        let _ = raven_libc_sys::sys::debug_write(
            alloc::format!(
                "dp2 pid={} old={} new={} ok={} err={}\n",
                // SAFETY: the C `getpid` export takes no arguments and reads no memory.
                unsafe { crate::compat::getpid() },
                old,
                new,
                result.is_ok(),
                result.as_ref().err().map_or(0, |e| e.0)
            )
            .as_bytes(),
        );
    }
    match result {
        Ok(value) => value,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
