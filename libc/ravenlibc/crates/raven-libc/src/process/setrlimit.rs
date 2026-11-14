// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setrlimit(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Rlimit;

/// Write one resource limit.
///
/// # Safety
/// `limit` points to a readable `struct rlimit`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setrlimit(resource: c_int, limit: *const Rlimit) -> c_int {
    if limit.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    let result =
        crate::ffi::map_proc::rlimit(&crate::ffi::procbackend::backend(), 0, resource, limit);
    crate::ffi::ret::int(result.map(|_| 0).map_err(crate::ffi::map_proc::error))
}
