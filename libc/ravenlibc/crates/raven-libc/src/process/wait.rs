// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wait(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Rusage;

/// Wait for any child.
///
/// # Safety
/// `status` is NULL or writable; `rusage` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wait(status: *mut c_int) -> c_int {
    let result = crate::ffi::map_proc::wait(
        &crate::ffi::procbackend::backend(),
        status,
        core::ptr::null_mut::<Rusage>(),
    );
    crate::ffi::ret::int(result.map_err(crate::ffi::map_proc::error))
}
