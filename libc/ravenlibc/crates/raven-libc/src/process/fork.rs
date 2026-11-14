// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fork(2)`.
use core::ffi::c_int;

/// Create a child process; `ENOSYS` until `procd` exists (H-18).
#[unsafe(no_mangle)]
pub extern "C" fn fork() -> c_int {
    let result = crate::ffi::map_proc::fork(&crate::ffi::procbackend::backend());
    crate::ffi::ret::int(result.map_err(crate::ffi::map_proc::error))
}
