// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getpgrp(2)`.
use core::ffi::c_int;
use raven_libc_proc::backend::ProcBackend;

/// The process group of the caller.
#[unsafe(no_mangle)]
pub extern "C" fn getpgrp() -> c_int {
    let result = crate::ffi::procbackend::backend().getpgid(0);
    crate::ffi::ret::int(result.map_err(crate::ffi::map_proc::error))
}
