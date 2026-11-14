// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `prlimit(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Rlimit;

/// Read and/or write one resource limit of `pid`.
///
/// # Safety
/// `new` is NULL or readable; `old` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn prlimit(
    pid: c_int,
    resource: c_int,
    new: *const Rlimit,
    old: *mut Rlimit,
) -> c_int {
    if pid < 0 {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::invalid()));
    }
    let result =
        crate::ffi::map_proc::rlimit(&crate::ffi::procbackend::backend(), pid, resource, new);
    match result {
        Ok(pair) => {
            if !old.is_null() {
                // SAFETY: the caller's writable limit record.
                unsafe { old.write(crate::ffi::map_proc::limit(pair)) };
            }
            0
        }
        Err(error) => crate::ffi::ret::int(Err(crate::ffi::map_proc::error(error))),
    }
}
