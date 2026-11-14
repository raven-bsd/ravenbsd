// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setpgid(2)`.
use core::ffi::c_int;
use raven_libc_proc::backend::ProcBackend;

/// Set the process group through the backend.
#[unsafe(no_mangle)]
pub extern "C" fn setpgid(pid: c_int, pgid: c_int) -> c_int {
    let result = crate::ffi::procbackend::backend().setpgid(pid, pgid);
    crate::ffi::ret::int(result.map(|()| 0).map_err(crate::ffi::map_proc::error))
}
