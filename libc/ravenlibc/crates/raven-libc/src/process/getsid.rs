// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getsid(2)`.
use core::ffi::c_int;
use raven_libc_proc::backend::ProcBackend;

/// The backend's answer for `pid`.
#[unsafe(no_mangle)]
pub extern "C" fn getsid(pid: c_int) -> c_int {
    let result = crate::ffi::procbackend::backend().getsid(pid);
    crate::ffi::ret::int(result.map_err(crate::ffi::map_proc::error))
}
