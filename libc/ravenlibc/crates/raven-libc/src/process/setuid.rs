// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setuid(2)`.
use core::ffi::c_int;
use raven_libc_proc::backend::ProcBackend;

/// Change the real and effective user id through the backend.
#[unsafe(no_mangle)]
pub extern "C" fn setuid(uid: u32) -> c_int {
    let request = raven_libc_proc::ids::SetCred::setuid(uid);
    crate::ffi::ret::int(
        crate::ffi::procbackend::backend()
            .setcred(&request)
            .map(|()| 0)
            .map_err(crate::ffi::map_proc::error),
    )
}
