// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getppid(2)`.
use core::ffi::c_int;
use raven_libc_proc::backend::ProcBackend;

/// The backend's value.
#[unsafe(no_mangle)]
pub extern "C" fn getppid() -> c_int {
    crate::ffi::procbackend::backend().getppid()
}
