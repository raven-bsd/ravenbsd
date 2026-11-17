// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `shm_unlink`: no POSIX shared-memory service exists, so the answer is `ENOSYS`.

use core::ffi::c_char;

use raven_libc_ipc::sysv;

use super::fail;

/// `shm_unlink`.
///
/// # Safety
/// `name` is a NUL-terminated POSIX shared-memory name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shm_unlink(name: *const c_char) -> core::ffi::c_int {
    if name.is_null() {
        return fail(raven_libc_ipc::EINVAL);
    }
    fail(sysv::unsupported())
}
