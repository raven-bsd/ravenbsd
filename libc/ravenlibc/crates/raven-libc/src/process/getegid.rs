// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getegid(2)`.
use raven_libc_proc::backend::ProcBackend;
/// The backend's value.
#[unsafe(no_mangle)]
pub extern "C" fn getegid() -> u32 {
    crate::ffi::procbackend::backend().getegid()
}
