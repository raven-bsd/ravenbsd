// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wait3(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Rusage;

/// Wait for any child and zero its usage record.
///
/// # Safety
/// `status` and `rusage` are NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wait3(status: *mut c_int, options: c_int, rusage: *mut Rusage) -> c_int {
    let result = crate::ffi::map_proc::wait_status(
        &crate::ffi::procbackend::backend(),
        raven_libc_proc::consts::P_ALL,
        0,
        options,
        status,
        rusage,
    );
    crate::ffi::ret::int(result.map_err(crate::ffi::map_proc::error))
}
