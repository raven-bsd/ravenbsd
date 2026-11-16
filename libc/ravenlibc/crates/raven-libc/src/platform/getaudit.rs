// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getaudit`.
use core::ffi::{c_int, c_void};

/// Audit information is not part of the Raven security model (registered stub).
///
/// # Safety
/// The pointer is not dereferenced.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getaudit(_info: *mut c_void) -> c_int {
    crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
        raven_libc_abi::errno::ENOSYS as u16,
    )))
}
