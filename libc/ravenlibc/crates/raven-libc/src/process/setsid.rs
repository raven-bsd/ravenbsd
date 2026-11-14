// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setsid(2)`.
use core::ffi::c_int;
use raven_libc_proc::backend::ProcBackend;

/// Create a session through the backend.
#[unsafe(no_mangle)]
pub extern "C" fn setsid() -> c_int {
    let result = crate::ffi::procbackend::backend().setsid();
    crate::ffi::ret::int(result.map_err(crate::ffi::map_proc::error))
}
