// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setpriority(2)`.
use core::ffi::c_int;
use raven_libc_proc::backend::ProcBackend;

/// Write one priority through the backend.
#[unsafe(no_mangle)]
pub extern "C" fn setpriority(which: c_int, who: c_int, priority: c_int) -> c_int {
    let Ok(which) = u32::try_from(which) else {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::invalid()));
    };
    let result = crate::ffi::procbackend::backend().prio(which, who, Some(priority));
    crate::ffi::ret::int(result.map(|_| 0).map_err(crate::ffi::map_proc::error))
}
