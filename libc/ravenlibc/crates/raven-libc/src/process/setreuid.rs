// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setreuid(2)`.
use core::ffi::c_int;
use raven_libc_proc::backend::ProcBackend;

/// Change the real and effective user ids through the backend.
#[unsafe(no_mangle)]
pub extern "C" fn setreuid(real: i32, effective: i32) -> c_int {
    let request = raven_libc_proc::ids::SetCred::setreuid(real, effective);
    crate::ffi::ret::int(
        crate::ffi::procbackend::backend()
            .setcred(&request)
            .map(|()| 0)
            .map_err(crate::ffi::map_proc::error),
    )
}
