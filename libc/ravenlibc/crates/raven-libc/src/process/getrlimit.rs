// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getrlimit(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Rlimit;

/// Read one resource limit.
///
/// # Safety
/// `limit` is NULL or points to a writable `struct rlimit`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getrlimit(resource: c_int, limit: *mut Rlimit) -> c_int {
    if limit.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    let result = crate::ffi::map_proc::rlimit(
        &crate::ffi::procbackend::backend(),
        0,
        resource,
        core::ptr::null(),
    );
    match result {
        // SAFETY: the caller's writable limit record.
        Ok(pair) => unsafe {
            limit.write(crate::ffi::map_proc::limit(pair));
            0
        },
        Err(error) => crate::ffi::ret::int(Err(crate::ffi::map_proc::error(error))),
    }
}
