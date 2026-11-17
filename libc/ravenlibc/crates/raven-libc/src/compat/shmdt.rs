// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `shmdt`: a null address is `EINVAL`; otherwise no SysV shared-memory service is `ENOSYS`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::sysv;

use super::fail;

/// `shmdt`.
///
/// # Safety
/// `address` is NULL or a previously returned mapping address.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shmdt(address: *const c_void) -> c_int {
    if address.is_null() {
        return fail(raven_libc_ipc::EINVAL);
    }
    fail(sysv::unsupported())
}
